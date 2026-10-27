//! The desktop shell: its own loop on `ljos_host` with egui-winit, and
//! egui's GL painter on EGL, egui-wgpu where that cannot be had, or the
//! processor (ARCHITECTURE.md §7.3), one [`Engine`] with QtQuick
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
/// The global allocator that lets the inspector tell what Rust code
/// holds from what C libraries do.
#[cfg(feature = "mcp")]
pub use ljos_dev::mcp::CountingAlloc;
#[cfg(feature = "dev")]
pub use ljos_dev::{TopRow, TopView};
#[cfg(feature = "mcp")]
pub use tools::{InspectorAccess, InspectorAddress, InspectorOptions};

#[cfg(target_os = "macos")]
mod appkit_drag;
mod badge;
mod cpu;
mod desktop;
mod desktop_font;
mod drags;
#[cfg(target_os = "linux")]
mod drivers;
#[cfg(gl_painter)]
mod gl;
#[cfg(gl_painter)]
mod gl_mesh;
#[cfg(gl_painter)]
mod gl_shader;
mod native_drag;
mod network;
mod platform;
mod present;
mod repaint;
mod roots;
mod screen_reader;
mod settings;
#[cfg(gl_painter)]
mod software;
mod surface;
mod system_fonts;
#[cfg(all(test, target_os = "linux"))]
mod test_bus;
mod wayland_drag;
#[cfg(target_os = "windows")]
mod win32_drag;
mod window_spec;
mod x11_drag;

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
    /// A root that is a plain item opens a window of its own width and
    /// height, as `qml6` does; `size` is for a side it leaves at 0.
    pub root_sized: bool,
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
    /// [`run`] starts the application in the system's locale and UI
    /// language, as the environment's locale categories name them
    /// (`LC_ALL`, `LC_MESSAGES`, `LC_NUMERIC`, `LC_TIME`, `LC_COLLATE`,
    /// `LANG`, `LANGUAGE`), before anything of the application runs, so
    /// that the application's own choice comes after it and wins. False
    /// keeps the engine's `en_US`, the same on every machine.
    pub system_locale: bool,
    /// [`run`] shows its windows through X11 rather than Wayland where the
    /// session has both (XWayland), for a compositor that gives the shell
    /// no drags from other applications. False keeps Wayland where there
    /// is one.
    /// The environment's `LJOS_PREFER_X11` (`1` or `0`) overrides it, for
    /// the user to choose either way.
    pub prefer_x11: bool,
    /// [`run`] draws its windows on the processor, into memory it shares
    /// with the window system (`wl_shm`, X11's MIT-SHM), with no graphics
    /// driver: the reference renderer draws what changed. A `ShaderEffect`'s
    /// own program and what egui draws (the inspector) are left out, and
    /// the log says so once. False draws with GL, or wgpu where there is
    /// no GL; where EGL would offer only a software rasteriser and the
    /// background is opaque, the shell draws on the processor itself, and
    /// goes to GL as soon as a window shows what it leaves out. The
    /// environment's `LJOS_PAINTER` (`cpu`, `gl` or `wgpu`) overrides it.
    pub drawn_on_processor: bool,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            title: "Ljós".to_owned(),
            size: [800.0, 600.0],
            root_sized: false,
            min_size: None,
            background: Color::WHITE,
            inbox_capacity: 1024,
            drain_budget: DrainBudget::time(Duration::from_millis(4)),
            system_fonts: true,
            system_locale: true,
            prefer_x11: false,
            drawn_on_processor: false,
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

    /// See [`ShellConfig::root_sized`].
    pub fn root_sized(mut self) -> Self {
        self.root_sized = true;
        self
    }

    pub fn with_background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    /// See [`ShellConfig::prefer_x11`].
    pub fn prefer_x11(mut self) -> Self {
        self.prefer_x11 = true;
        self
    }

    /// See [`ShellConfig::drawn_on_processor`].
    pub fn drawn_on_processor(mut self) -> Self {
        self.drawn_on_processor = true;
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

    /// Installs an application-wide key filter: `filter` sees every key
    /// event a window is given (presses, auto-repeats, releases) before
    /// its shortcuts and focused item, with the window's root, and true
    /// keeps it. See [`ljos_quick::keys::add_key_filter`].
    pub fn add_key_filter(
        &mut self,
        filter: impl Fn(&Engine, ObjectId, &ljos_quick::KeyEvent) -> bool + 'static,
    ) -> ljos_quick::keys::KeyFilterId {
        ljos_quick::keys::add_key_filter(self.engine, filter)
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
    /// Wakes the loop for the root's window without going through egui,
    /// which the loop's own passes do not answer for
    /// ([`QmlView::set_waker`]).
    pub(crate) wake: ljos_egui::Waker,
    /// The host speaks Wayland, which `Qt.platform.pluginName` says.
    pub(crate) wayland: bool,
}

impl<M: Send + 'static> ShellApp<M> {
    /// Creates the Engine, registers QtQuick, runs `setup` to build the
    /// root item and wires the inbox to wake `ctx`. `ctx` is given the
    /// bundled font family for what egui itself writes (the dev overlay).
    pub fn new(
        ctx: &egui::Context,
        config: &ShellConfig,
        clock: Arc<dyn Clock>,
        setup: impl FnOnce(&mut Setup<'_, M>) -> Result<ObjectId, SetupError>,
    ) -> Result<Self, ShellError> {
        // eframe is built without its defaults, which leaves egui with no
        // fonts; the shell's own loop gives its contexts the family too.
        ctx.set_fonts(ljos_egui::font_definitions());
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
        let (tree, waker, changed, wayland) = match hosting {
            Some(h) => {
                let changed: Rc<dyn Fn()> = Rc::from(h.changed);
                let windows = Rc::clone(&changed);
                ljos_controls::WindowSystem::of(&engine).set_host(move || windows());
                (Some(h.tree), Some(h.wake), Some(changed), h.wayland)
            }
            None => (None, None, None, false),
        };
        let wake: Arc<dyn Fn() + Send + Sync> = match &waker {
            Some(waker) => {
                let waker = Arc::clone(waker);
                Arc::new(move || waker(std::time::Duration::ZERO))
            }
            None => {
                let ctx = ctx.clone();
                Arc::new(move || ctx.request_repaint())
            }
        };
        {
            // A root the application asks for is shown before the next
            // frame, and before the platform's windows are made to match.
            let wake = Arc::clone(&wake);
            ljos_quick::roots::RootSwaps::of(&engine).set_wake(move || {
                if let Some(changed) = &changed {
                    changed();
                }
                wake();
            });
        }
        if tree.is_some() && config.system_locale {
            engine.use_system_locale();
        }
        if tree.is_some() {
            // As Qt does, before the root: the Material theme and colours
            // the configuration file and its variables name.
            ljos_controls::set_material_defaults(
                &engine,
                ljos_controls::MaterialDefaults::from_environment(),
            );
        }
        if tree.is_some() {
            // Before `setup` too: a `Settings` reads its file, and a
            // binding may ask `Qt.platform`, while the root is built.
            let wake = Arc::clone(&wake);
            platform::install(&engine, wayland, move || wake());
            // Pictures fill in after the window is drawn, as they do in
            // Qt: their fetches and decodes never hold up a frame.
            ljos_quick::ImageStore::of(&engine).set_start_after_frame(true);
        }
        let (inbox, handle) = UiInbox::channel_with_wake(config.inbox_capacity, Arc::clone(&wake));
        // The application's own messages: their kinds are its business.
        inbox.observe_as("application", None);
        let mut s = Setup {
            engine: &engine,
            handle,
            clock: Arc::clone(&clock),
            on_message: None,
            timer_hook: None,
            #[cfg(feature = "dev")]
            dev: dev::DevSetup::new(Arc::clone(&wake)),
            #[cfg(feature = "dev")]
            tools: {
                let [r, g, b, _] = config.background.to_rgba8();
                tools::ToolSetup::new(Arc::clone(&wake), [r, g, b])
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
        ljos_quick::roots::RootSwaps::of(&engine).shows(None, root);
        if let Some(tree) = tree {
            view = view.with_accessibility_graft(tree);
        }
        if let Some(waker) = waker {
            view.set_waker(waker);
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
        self.swap_roots();
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
        #[cfg(feature = "dev")]
        self.tell_shown();
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
    // Before anything starts a thread; done already when the fonts were
    // read early.
    #[cfg(target_os = "linux")]
    drivers::choose();
    desktop::run(config, setup)
}

/// Starts reading the system's fonts on a thread of their own, so that
/// what a program does before [`run`], such as loading or compiling its
/// QML, runs meanwhile. [`run`] starts it itself when it has not been
/// started, and with [`ShellConfig::system_fonts`] off does not wait for it.
///
/// It chooses the graphics drivers first, as [`run`] would, since that
/// sets the environment, which must not change once the reader starts.
/// Call it on the main thread before starting threads of your own.
pub fn read_system_fonts_early() {
    system_fonts::start();
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

/// Gives the memory the allocator's heaps hold free back to the system.
/// glibc returns only what is free at the top of a heap by itself; what
/// was freed below something still live stays resident. Start-up (shader
/// compiles, the first layout) and a burst of picture decodes leave such
/// holes: about 1.5–2 MiB of a settled window on llvmpipe. Costs a walk of
/// the heaps, so it is done after start-up, after a burst of picture loads
/// and under memory pressure, never per frame.
pub(crate) fn give_back_freed_heap() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: malloc_trim takes the allocator's locks itself.
    unsafe {
        libc::malloc_trim(0);
    }
}

/// Locks `m` whether or not a thread went down holding it: what the
/// shell's threads share is whole between their steps, and a panic on
/// one of them is not to stop the others.
pub(crate) fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(all(test, target_os = "linux", target_env = "gnu"))]
mod tests {
    /// Resident bytes of this process.
    fn resident() -> usize {
        let statm = std::fs::read_to_string("/proc/self/statm").unwrap();
        let pages: usize = statm.split_whitespace().nth(1).unwrap().parse().unwrap();
        // SAFETY: sysconf only reads a constant of the system.
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        pages * usize::try_from(page).unwrap()
    }

    #[test]
    fn memory_freed_below_live_memory_goes_back_to_the_system() {
        const BLOCK: usize = 16 << 10;
        const FREED: usize = 32 << 20;
        let blocks: Vec<Box<[u8]>> = (0..FREED / BLOCK)
            .map(|i| vec![u8::try_from(i % 251).unwrap() | 1; BLOCK].into_boxed_slice())
            .collect();
        // Live above what is freed, so that the heap cannot shrink from
        // its top by itself.
        let above = std::hint::black_box(vec![1u8; BLOCK]);
        drop(std::hint::black_box(blocks));
        let before = resident();
        super::give_back_freed_heap();
        let saved = before.saturating_sub(resident());
        assert!(
            saved >= FREED * 3 / 4,
            "{saved} of {FREED} freed bytes given back"
        );
        drop(above);
    }
}
