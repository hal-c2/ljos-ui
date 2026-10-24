//! Pictures of an application without a window (the non-default
//! `render` feature): a screenshot test, a visual regression check, a
//! picture for documentation.
//!
//! A [`Picture`] runs the Engine on a virtual clock, lets a stated time
//! pass on it — every timer fired at its own deadline, the scene
//! settled in between, as a host woken at each deadline would — and
//! draws the scene with the CPU reference renderer to a PNG. It is
//! what `ljos render` does after the project is loaded. What the scene
//! was promised and is still waiting for — a picture being decoded, a
//! fetch, a file read off the Engine thread — takes real time the virtual
//! clock knows nothing of, so it is waited for (within a bound) before
//! any virtual time passes and before the picture is taken.
//!
//! ```no_run
//! # mod my_module { pub fn register(_: &ljos::Engine) {} }
//! # fn components() -> ljos::MapSource { todo!() }
//! let picture = ljos::render::Picture::new(640, 420).settle(500);
//! // Before the Engine is built: text is laid out the same everywhere.
//! ljos::render::set_fonts(ljos::render::Fonts::Bundled);
//! let engine = ljos::App::new("Hello")
//!     .register(my_module::register)
//!     .source(components())
//!     .build_engine();
//! // The clock first, so the root's timers and animations start on it.
//! picture.install(&engine);
//! let root = engine.create_from_url("Main", None, Vec::new()).unwrap();
//! let png: Vec<u8> = picture.png(&engine, root).unwrap();
//! ```
//!
//! [`render_png`] is the same for the `(Engine, root)` that
//! [`App::build`](crate::App::build) returns, with the time of the call
//! as the clock's start.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ljos_engine::{Engine, ObjectId, QmlType as _, Value};
use ljos_quick::{QuickScene, TextEngine};

use crate::drive::Run;
use ljos_render::{FrameView, RasterConfig, Rgba, VelloCpuReference, kurbo};
use ljos_runtime::{Clock as _, VirtualClock};

/// The fonts text is set in on this thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fonts {
    /// The bundled font only: the picture is the same on every machine.
    #[default]
    Bundled,
    /// The fonts installed on the machine, with the platform's
    /// substitute for a family it lacks, as a desktop application sets
    /// text.
    System,
}

/// Chooses the fonts text is set in on this thread. Call it before the
/// Engine is built, so that nothing has measured text yet; until it is
/// called the fonts are [`Fonts::Bundled`].
pub fn set_fonts(fonts: Fonts) {
    ljos_quick::set_text_engine(match fonts {
        Fonts::Bundled => TextEngine::new(),
        Fonts::System => TextEngine::with_system_fonts(),
    });
}

/// Draws `root` at `size` (logical pixels) after `settle_ms` of
/// application time, as [`Picture::png`] does. The virtual clock starts
/// at the time of the call; install a [`Picture`] before the root is
/// made for its timers and animations to start on it too.
pub fn render_png(
    engine: &Engine,
    root: ObjectId,
    size: (u32, u32),
    settle_ms: u64,
) -> Result<Vec<u8>, String> {
    let picture = Picture::new(size.0, size.1).settle(settle_ms);
    picture.install(engine);
    picture.png(engine, root)
}

/// Hears the scene's messages; shared with the scene's log handler.
type SceneMessages = Rc<RefCell<dyn FnMut(&str)>>;

/// What a picture is taken at: a size, a time to let pass, and the
/// virtual clock the Engine runs on meanwhile.
pub struct Picture {
    size: (u32, u32),
    settle_ms: u64,
    clock: Arc<VirtualClock>,
    scene_messages: Option<SceneMessages>,
    window_colour: bool,
    root_sized: bool,
}

impl std::fmt::Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Picture")
            .field("size", &self.size)
            .field("settle_ms", &self.settle_ms)
            .finish_non_exhaustive()
    }
}

impl Picture {
    /// A picture `width` by `height` logical pixels, taken as soon as
    /// the scene has come to rest.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            size: (width, height),
            settle_ms: 0,
            clock: Arc::new(VirtualClock::new()),
            scene_messages: None,
            window_colour: true,
            root_sized: false,
        }
    }

    /// Milliseconds of application time to let pass before the
    /// picture: timers fire and animations run on the virtual clock.
    #[must_use]
    pub fn settle(mut self, ms: u64) -> Self {
        self.settle_ms = ms;
        self
    }

    /// Hears what the scene reports while it is laid out and drawn (a
    /// binding loop in layout, a picture that did not load), one line
    /// a message. The Engine's own messages go to its log handler.
    #[must_use]
    pub fn on_scene_message(mut self, f: impl FnMut(&str) + 'static) -> Self {
        self.scene_messages = Some(Rc::new(RefCell::new(f)));
        self
    }

    /// Leaves a `Window` root's own colour out of the picture, once
    /// the settle time has passed: what is left is its items, on
    /// nothing where no item paints — a picture to compare with one
    /// taken of the items alone.
    #[must_use]
    pub fn without_window_colour(mut self) -> Self {
        self.window_colour = false;
        self
    }

    /// Takes each side the root says, its own width and height or else
    /// its implicit ones, as `qml6` opens a window as large as its root;
    /// the picture's size is for a side the root leaves at 0.
    #[must_use]
    pub fn root_sized(mut self) -> Self {
        self.root_sized = true;
        self
    }

    /// Runs `engine` on this picture's virtual clock from now on, which
    /// stands still until [`png`](Self::png) lets time pass. Install it
    /// before the root is made for the root's timers and animations to
    /// start on it; a script's `Date` keeps the wall clock unless the
    /// Engine's wall-clock origin is pinned.
    pub fn install(&self, engine: &Engine) {
        self.clock.set(engine.now());
        engine.set_clock(Rc::new(Arc::clone(&self.clock)));
    }

    /// Lays `root` out at the picture's size, lets the settle time pass
    /// and draws it: an item on white, as a window with no colour of
    /// its own shows it, and a `Window` with its own colour. A root
    /// window that leaves its size unsaid is as large as the picture.
    /// `Err` when the scene does not come to rest or cannot be drawn.
    pub fn png(&self, engine: &Engine, root: ObjectId) -> Result<Vec<u8>, String> {
        let (mut width, mut height) = self.size;
        let mut scene = QuickScene::new(kurbo::Size::new(f64::from(width), f64::from(height)));
        if let Some(f) = &self.scene_messages {
            let f = Rc::clone(f);
            scene.log().set_handler(move |e| {
                (f.borrow_mut())(e.message.lines().next().unwrap_or("").trim())
            });
        }
        scene.set_root(engine, root).map_err(|e| e.to_string())?;
        if self.root_sized {
            // Laid out first, as a Text knows its implicit size only then.
            scene.lay_out(engine);
            let side = |name, or| {
                let n = engine.read_by_name(root, name).to_number();
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                if n >= 1.0 { n.ceil() as u32 } else { or }
            };
            width = side("width", width);
            height = side("height", height);
            scene.set_viewport(kurbo::Rect::new(
                0.0,
                0.0,
                f64::from(width),
                f64::from(height),
            ));
        }
        // As the shell gives such a window the size of its platform
        // window: without it its content item is empty, and so is
        // whatever is sized from it (a popup's `parent.width`).
        let unsaid = |name| engine.read_by_name(root, name).to_number() <= 0.0;
        if unsaid("width") || unsaid("height") {
            let size = [f64::from(width), f64::from(height)];
            ljos_controls::WindowSystem::of(engine).resized(engine, root, size);
        }
        let mut wake = None;
        let mut run = Run {
            engine,
            scene: &mut scene,
            clock: &self.clock,
            wake: &mut wake,
            serve: None,
        };
        run.settle()?;
        run.advance(self.settle_ms)?;
        let window = engine
            .meta_of(root)
            .is_some_and(|m| m.inherits(ljos_controls::Window::static_meta()));
        if window && !self.window_colour {
            // A window is a Rectangle underneath: its fill goes.
            engine
                .write_by_name(root, "color", Value::from("transparent"))
                .map_err(|e| e.to_string())?;
        }
        picture(&mut run, window)
    }
}

/// A PNG of the scene: on white, or on nothing for a window, which
/// paints its own colour.
fn picture(run: &mut Run<'_>, window: bool) -> Result<Vec<u8>, String> {
    run.settle()?;
    let out = run.scene.frame(run.engine, run.clock.now());
    let backdrop = if window { 0.0 } else { 1.0 };
    let mut r = VelloCpuReference::new(RasterConfig {
        background: Rgba::from_straight(backdrop, backdrop, backdrop, backdrop),
        ..RasterConfig::default()
    });
    let image = r
        .render(&FrameView::new(&out.snapshot, 1.0))
        .map_err(|e| e.to_string())?;
    image.to_png().map_err(|e| e.to_string())
}
