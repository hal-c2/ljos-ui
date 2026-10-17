//! The desktop shell: its own loop on `ljos_host` with egui-winit and
//! egui-wgpu (ARCHITECTURE.md §7.3), one [`Engine`] with QtQuick
//! registered, a platform window for the root and one for every QML
//! `Window` that is shown, and the UI inbox through which other threads
//! talk to the Engine (§7.2 step 1; principle: threads talk to the Engine
//! through `UiHandle` only).
//!
//! The loop sleeps until something asks for a pass: input, a timer, an
//! animation, a message or dirty state. The shell owns each window's
//! surface (presented with vsync), and its AccessKit adapter, into whose
//! tree of egui the scene's own tree is grafted (§14).
//!
//! ```no_run
//! use ljos_shell::{ShellConfig, run};
//! # fn build(_: &ljos_engine::Engine) -> ljos_engine::ObjectId { unimplemented!() }
//! run::<()>(ShellConfig::new("Hello"), |setup| Ok(build(setup.engine()))).unwrap();
//! ```

#[cfg(feature = "dev")]
mod dev;
#[cfg(feature = "dev")]
pub use ljos_dev::{Overlay, ReloadStats, Report};
#[cfg(feature = "dev")]
mod tools;
#[cfg(feature = "dev")]
pub use ljos_dev::{TopRow, TopView};
#[cfg(feature = "mcp")]
pub use tools::{InspectorAccess, InspectorAddress, InspectorOptions};

mod badge;
mod desktop;
#[cfg(gl_painter)]
mod gl;
mod platform;
mod present;
mod repaint;
mod surface;
mod windows;

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ljos_egui::{QmlView, ViewError};
use ljos_engine::{Color, Engine, ObjectId};
use ljos_quick::TimerHook;
use ljos_runtime::{Clock, DrainBudget, DrainReport, UiHandle, UiInbox};

/// Window and pipeline settings. A root that is a `Window` (or an
/// `ApplicationWindow`) says its own title, size, limits and colour; what
/// is here is for a root that is a plain item, and for the title and the
/// size such a window leaves unsaid.
#[derive(Clone, Debug)]
pub struct ShellConfig {
    pub title: String,
    /// Initial inner size in logical pixels.
    pub size: [f32; 2],
    pub min_size: Option<[f32; 2]>,
    /// Painted behind a root that is a plain item (QQuickWindow's default
    /// is white). A root `Window` paints its own `color`.
    pub background: Color,
    /// Bound on queued messages; a full inbox pushes back on senders.
    pub inbox_capacity: usize,
    /// How much of a frame the inbox drain may take; the rest waits for the
    /// next frame (which the inbox requests itself).
    pub drain_budget: DrainBudget,
    /// [`run`] lays text out with the fonts installed on the system, and the
    /// platform's interface font when a `Text` names no family. False keeps
    /// the bundled font only, so text has the same size on every machine.
    ///
    /// Only [`run`] reads this. A host that builds a [`ShellApp`] itself
    /// keeps the text engine its thread already has (bundled fonts, unless
    /// it called [`ljos_quick::set_text_engine`]).
    pub system_fonts: bool,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            title: "Ljós".to_owned(),
            size: [800.0, 600.0],
            min_size: None,
            background: Color::WHITE,
            inbox_capacity: 1024,
            drain_budget: DrainBudget::time(Duration::from_millis(4)),
            system_fonts: true,
        }
    }
}

impl ShellConfig {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.size = [width, height];
        self
    }

    pub fn with_background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }
}

/// What a setup closure may fail with.
pub type SetupError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error("window: {0}")]
    Window(#[from] ljos_host::HostError),
    /// No surface to draw the first window into.
    #[error("graphics: {0}")]
    Gpu(String),
    #[error("setup: {0}")]
    Setup(SetupError),
    #[error(transparent)]
    View(#[from] ViewError),
}

type MessageHandler<M> = Box<dyn FnMut(&Engine, M)>;

/// Handed to the setup closure: the Engine to build the root in, the
/// thread-safe handle for posting messages, and hooks.
pub struct Setup<'a, M> {
    engine: &'a Rc<Engine>,
    handle: UiHandle<M>,
    clock: Arc<dyn Clock>,
    on_message: Option<MessageHandler<M>>,
    timer_hook: Option<TimerHook>,
    #[cfg(feature = "dev")]
    dev: dev::DevSetup,
    #[cfg(feature = "dev")]
    tools: tools::ToolSetup,
}

impl<M> Setup<'_, M> {
    /// QtQuick is already registered.
    pub fn engine(&self) -> &Rc<Engine> {
        self.engine
    }

    /// Clone this into worker threads; every `send` wakes the UI.
    pub fn handle(&self) -> UiHandle<M> {
        self.handle.clone()
    }

    pub fn clock(&self) -> &Arc<dyn Clock> {
        &self.clock
    }

    /// The font families the windows will lay text out with, sorted:
    /// under [`run`] the [`ShellConfig::system_fonts`] choice is already
    /// applied, so a family picked from here is one the text will use.
    /// A host that builds a [`ShellApp`] itself gets its thread's text
    /// engine (see [`ljos_quick::font_families`]).
    pub fn font_families(&self) -> Vec<String> {
        ljos_quick::font_families()
    }

    /// Runs on the UI thread for every message, at the start of a frame.
    /// Each message is a turn of its own ([`Engine::turn`]): a panic in
    /// the handler reaches the engine's fault handler and the next
    /// message is still handled.
    pub fn on_message(&mut self, f: impl FnMut(&Engine, M) + 'static) {
        self.on_message = Some(Box::new(f));
    }

    /// See [`ljos_quick::TimerHook`]; times come from [`Setup::clock`].
    pub fn set_timer_hook(&mut self, hook: TimerHook) {
        self.timer_hook = Some(hook);
    }
}

impl<M> std::fmt::Debug for Setup<'_, M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Setup")
            .field("on_message", &self.on_message.is_some())
            .field("timer_hook", &self.timer_hook.is_some())
            .finish_non_exhaustive()
    }
}

/// What [`run`] shows: the Engine, the view of its root, the inbox and
/// the development tools. Public so that tests and hosts with an egui
/// loop of their own can build one from an `egui::Context` and run its
/// passes ([`ShellApp::begin_pass`], [`ShellApp::show`]); with the feature
/// `eframe` it is an `eframe::App`. Shown that way a QML `Window` is an
/// item of the scene: only [`run`] gives each a platform window.
pub struct ShellApp<M> {
    view: QmlView,
    inbox: UiInbox<M>,
    on_message: Option<MessageHandler<M>>,
    budget: DrainBudget,
    background: egui::Color32,
    /// Whether [`ShellApp::show`] paints the background: not behind a
    /// root that is a window of its own, which paints its `color`.
    backdrop: bool,
    last_drain: DrainReport,
    /// The watched application of a development build.
    #[cfg(feature = "dev")]
    dev: Option<dev::Dev>,
    /// The `top` overlay and the inspector of a development build.
    #[cfg(feature = "dev")]
    tools: tools::Tools,
}

impl<M> std::fmt::Debug for ShellApp<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShellApp")
            .field("view", &self.view)
            .field("queued", &self.inbox.len())
            .field("last_drain", &self.last_drain)
            .finish_non_exhaustive()
    }
}

/// What [`run`] adds to a [`ShellApp`]: platform windows.
pub(crate) struct Hosting {
    /// The AccessKit tree of the root's scene, grafted into egui's.
    pub(crate) tree: egui::accesskit::TreeId,
    /// Called when a QML `Window` wants something else of its platform
    /// window than it has.
    pub(crate) changed: Box<dyn Fn()>,
}

impl<M: Send + 'static> ShellApp<M> {
    /// Creates the Engine, registers QtQuick, runs `setup` to build the
    /// root item and wires the inbox to wake `ctx`.
    pub fn new(
        ctx: &egui::Context,
        config: &ShellConfig,
        clock: Arc<dyn Clock>,
        setup: impl FnOnce(&mut Setup<'_, M>) -> Result<ObjectId, SetupError>,
    ) -> Result<Self, ShellError> {
        Self::build(ctx, config, clock, setup, None)
    }

    /// As [`ShellApp::new`], for the shell's own loop: every QML `Window`
    /// of the engine is a platform window of `hosting`.
    pub(crate) fn hosted(
        ctx: &egui::Context,
        config: &ShellConfig,
        clock: Arc<dyn Clock>,
        setup: impl FnOnce(&mut Setup<'_, M>) -> Result<ObjectId, SetupError>,
        hosting: Hosting,
    ) -> Result<Self, ShellError> {
        Self::build(ctx, config, clock, setup, Some(hosting))
    }

    fn build(
        ctx: &egui::Context,
        config: &ShellConfig,
        clock: Arc<dyn Clock>,
        setup: impl FnOnce(&mut Setup<'_, M>) -> Result<ObjectId, SetupError>,
        hosting: Option<Hosting>,
    ) -> Result<Self, ShellError> {
        let engine = Rc::new(Engine::new());
        ljos_quick::register(&engine);
        ljos_controls::register(&engine);
        #[cfg(feature = "media")]
        ljos_media::register(&engine);
        // Before `setup`: a window knows from its first moment whether it
        // is an item of a scene or a window of the platform.
        let tree = hosting.map(|h| {
            ljos_controls::WindowSystem::of(&engine).set_host(h.changed);
            h.tree
        });
        if tree.is_some() {
            // Before `setup` too: a `Settings` reads its file, and a
            // binding may ask `Qt.platform`, while the root is built.
            let wake_ctx = ctx.clone();
            platform::install(&engine, move || wake_ctx.request_repaint());
            // Pictures fill in after the window is drawn, as they do in
            // Qt: their fetches and decodes never hold up a frame.
            ljos_quick::ImageStore::of(&engine).set_start_after_frame(true);
        }
        let wake_ctx = ctx.clone();
        let (inbox, handle) = UiInbox::channel_with_wake(
            config.inbox_capacity,
            Arc::new(move || wake_ctx.request_repaint()),
        );
        let mut s = Setup {
            engine: &engine,
            handle,
            clock: Arc::clone(&clock),
            on_message: None,
            timer_hook: None,
            #[cfg(feature = "dev")]
            dev: dev::DevSetup::new(ctx),
            #[cfg(feature = "dev")]
            tools: {
                let ctx = ctx.clone();
                let [r, g, b, _] = config.background.to_rgba8();
                tools::ToolSetup::new(Arc::new(move || ctx.request_repaint()), [r, g, b])
            },
        };
        let root = setup(&mut s).map_err(ShellError::Setup)?;
        let Setup {
            on_message,
            timer_hook,
            #[cfg(feature = "dev")]
            dev,
            #[cfg(feature = "dev")]
            tools,
            ..
        } = s;
        let mut view = QmlView::new(Rc::clone(&engine), root)?.with_clock(clock);
        if let Some(tree) = tree {
            view = view.with_accessibility_graft(tree);
        }
        view.bind_context(ctx);
        if let Some(h) = timer_hook {
            view.set_timer_hook(h);
        }
        let [r, g, b, a] = config.background.to_rgba8();
        Ok(Self {
            view,
            inbox,
            on_message,
            budget: config.drain_budget,
            background: egui::Color32::from_rgba_unmultiplied(r, g, b, a),
            backdrop: true,
            last_drain: DrainReport::default(),
            #[cfg(feature = "dev")]
            dev: dev.finish(),
            #[cfg(feature = "dev")]
            tools: tools.finish(),
        })
    }

    pub fn handle(&self) -> UiHandle<M> {
        self.inbox.handle()
    }

    pub fn view(&self) -> &QmlView {
        &self.view
    }

    pub fn view_mut(&mut self) -> &mut QmlView {
        &mut self.view
    }

    pub fn engine(&self) -> &Rc<Engine> {
        self.view.engine()
    }

    /// The most recent inbox drain.
    pub fn last_drain(&self) -> DrainReport {
        self.last_drain
    }

    /// Pipeline step 1: deliver queued messages. The view's waker is held
    /// because the pass that follows shows whatever the handlers changed.
    pub fn drain_inbox(&mut self) -> DrainReport {
        let Self {
            view,
            inbox,
            on_message,
            budget,
            ..
        } = self;
        let report = view.without_wake(|view| {
            let engine = Rc::clone(view.engine());
            let clock = Arc::clone(view.clock());
            inbox.drain(*budget, &*clock, |m| {
                if let Some(f) = on_message.as_mut() {
                    // One message, one turn: a handler that panics is a
                    // fault of the application, not the end of it, and
                    // the messages after it are still handled.
                    engine.turn(None, || f(&engine, m));
                }
            })
        });
        self.last_drain = report;
        report
    }
}

impl<M: Send + 'static> ShellApp<M> {
    /// Pipeline step 1, at the start of a pass and before
    /// [`ShellApp::show`]: the inbox, then what the watcher and the
    /// inspector of a development build brought.
    pub fn begin_pass(&mut self) {
        #[cfg(feature = "dev")]
        self.begin_tools();
        self.drain_inbox();
        #[cfg(feature = "dev")]
        self.pump_dev();
        #[cfg(feature = "dev")]
        self.pump_tools();
    }

    /// The rest of a pass: the scene over all of `ui`, and the overlays
    /// of a development build on top.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        if self.backdrop {
            ui.painter()
                .rect_filled(ui.max_rect(), 0.0, self.background);
        }
        self.view.show(ui);
        #[cfg(feature = "dev")]
        self.show_dev(ui);
        #[cfg(feature = "dev")]
        self.end_tools(ui);
    }

    /// Whether the background is painted behind the root.
    pub(crate) fn set_backdrop(&mut self, on: bool) {
        self.backdrop = on;
    }
}

#[cfg(feature = "eframe")]
impl<M: Send + 'static> eframe::App for ShellApp<M> {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.begin_pass();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

/// Opens the application's windows and runs until the last of them is
/// closed or the application quits (`Qt.quit()`). `setup` builds the root
/// item in the given Engine.
///
/// A root that is a plain item is shown in a window made from `config`.
/// A root `Window` is that window, and like every other `Window` it is
/// on the screen while its `visible` is true.
///
/// Call it on the main thread: some platforms allow a loop nowhere else.
pub fn run<M: Send + 'static>(
    config: ShellConfig,
    setup: impl FnOnce(&mut Setup<'_, M>) -> Result<ObjectId, SetupError> + 'static,
) -> Result<(), ShellError> {
    large_buffers_from_the_system();
    desktop::run(config, setup)
}

/// Buffers of a mebibyte or more (decoded pictures and the scratch they
/// are made in) are mapped from the system and given back when freed.
/// glibc raises that threshold each time such a buffer is freed, after
/// which they come from the heaps of the threads that decode, which keep
/// the memory: on rssnews a quarter of a gigabyte of pictures long gone
/// stayed resident. Setting the threshold stops it moving.
fn large_buffers_from_the_system() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: mallopt only sets a parameter of the allocator, under its
    // own lock.
    unsafe {
        libc::mallopt(libc::M_MMAP_THRESHOLD, 1 << 20);
    }
}
