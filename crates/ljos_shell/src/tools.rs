//! Development tools in the shell (feature `dev`): the `top` overlay
//! (ARCHITECTURE.md §3.4, §19.10) and, with the feature `mcp`, the
//! inspector an agent connects to (§19.4).
//!
//! Neither keeps the window awake. The overlay is given the report of a
//! pass in which the scene ran a frame anyway and is painted in that same
//! pass; it never asks for one. The inspector's server has a thread of
//! its own that sleeps until a client writes, and only then asks egui for
//! a repaint, which is the pass that answers. A window with the overlay
//! hidden and no inspector pays one branch a pass.

use std::time::Instant;

use ljos_dev::{Top, TopView};
use ljos_runtime::WakeHook;

use crate::{Setup, ShellApp};

/// What `Setup` holds for the tools until the window is made.
pub(crate) struct ToolSetup {
    wake: WakeHook,
    top: bool,
    #[cfg(feature = "mcp")]
    background: [u8; 3],
    #[cfg(feature = "mcp")]
    inspector: Option<inspector::Served>,
}

impl ToolSetup {
    pub(crate) fn new(wake: WakeHook, background: [u8; 3]) -> Self {
        #[cfg(not(feature = "mcp"))]
        let _ = background;
        Self {
            wake,
            top: false,
            #[cfg(feature = "mcp")]
            background,
            #[cfg(feature = "mcp")]
            inspector: None,
        }
    }

    pub(crate) fn finish(self) -> Tools {
        let mut top = Top::new();
        top.set_visible(self.top);
        Tools {
            top,
            wake: self.wake,
            frames: 0,
            started: None,
            #[cfg(feature = "mcp")]
            inspector: self.inspector,
        }
    }
}

/// The tools of a [`ShellApp`].
pub(crate) struct Tools {
    top: Top,
    /// Asks egui for one pass.
    wake: WakeHook,
    /// The scene's frame count when a frame was last recorded: a pass
    /// that egui ran for a reason of its own is not a frame of the
    /// application.
    frames: u64,
    /// When this pass began, while something measures.
    started: Option<Instant>,
    #[cfg(feature = "mcp")]
    inspector: Option<inspector::Served>,
}

impl Tools {
    fn measuring(&self) -> bool {
        #[cfg(feature = "mcp")]
        if self.inspector.is_some() {
            return true;
        }
        self.top.visible()
    }
}

impl<M> Setup<'_, M> {
    /// Shows the `top` overlay from the first frame: the time of the last
    /// frame by lane and by owner. [`ShellApp::set_top_visible`] shows
    /// and hides it later.
    pub fn show_top(&mut self) {
        self.tools.top = true;
    }
}

impl<M> ShellApp<M> {
    /// What the `top` overlay shows; `None` while it is hidden.
    pub fn top(&self) -> Option<TopView> {
        self.tools.top.view()
    }

    pub fn top_visible(&self) -> bool {
        self.tools.top.visible()
    }

    /// Shows or hides the `top` overlay. Hidden, it records nothing.
    pub fn set_top_visible(&mut self, visible: bool) {
        if self.tools.top.visible() != visible {
            if visible && !self.tools.measuring() {
                // Nothing took the reports of the frames so far. What one
                // of them cost is not the cost of the first frame shown.
                let _ = self.view.scene_mut().take_incubation_report();
            }
            self.tools.top.set_visible(visible);
            // One pass, to paint it or to paint the window without it.
            (self.tools.wake)();
        }
    }

    /// Start of a pass: notes the time while something measures.
    pub(crate) fn begin_tools(&mut self) {
        let tools = &mut self.tools;
        tools.started = tools.measuring().then(Instant::now);
    }

    /// Pipeline step 1, after the inbox and the watcher: answers what an
    /// agent asked since the last pass. The view's waker is held, as for
    /// the inbox: a request that looks at the scene leaves it asking for
    /// a frame, and the pass that follows is that frame.
    pub(crate) fn pump_tools(&mut self) {
        #[cfg(feature = "mcp")]
        {
            let watched = self.is_watched();
            let Self { view, tools, .. } = self;
            if let Some(served) = tools.inspector.as_mut() {
                served.set_watched(watched);
                view.without_wake(|view| served.pump(view));
            }
            // An agent asked for a reload: the watcher reads the files at
            // once and wakes this window when it has built them. Its answer
            // is given in the pass that applied the result.
            let asked = tools
                .inspector
                .as_mut()
                .is_some_and(|served| served.take_reload_request());
            if asked {
                self.reload_now();
            }
            if self.take_flushed() {
                let Self { view, tools, .. } = self;
                if let Some(served) = tools.inspector.as_mut() {
                    served.finish_reload(view);
                }
            }
        }
    }

    /// End of a pass: records the frame the scene ran, if it ran one, and
    /// paints the overlay.
    pub(crate) fn end_tools(&mut self, ui: &mut egui::Ui) {
        let Self {
            view,
            tools,
            last_drain,
            ..
        } = self;
        // An agent's looking in this pass. In a pass without a frame of
        // the application (the log was read) it is in no frame.
        #[cfg(feature = "mcp")]
        let looked = tools.inspector.as_mut().and_then(|s| s.take_work());
        #[cfg(feature = "mcp")]
        if let Some(served) = tools.inspector.as_mut() {
            served.record_scene_frames(view.scene_mut(), last_drain.drained);
        }
        let frames = view.stats().frames;
        if tools.measuring() && frames != tools.frames {
            tools.frames = frames;
            // The scheduled work of the frame: creation jobs by lane and
            // owner, and the inbox. The rest of the frame (bindings,
            // layout, painting) is in the frame's time only.
            let mut report = view
                .scene_mut()
                .take_incubation_report()
                .unwrap_or_default();
            report.record_drain(last_drain);
            if let Some(started) = tools.started {
                report.elapsed = started.elapsed();
            }
            // The inspector counts its own work as it does it.
            #[cfg(feature = "mcp")]
            if let Some(served) = tools.inspector.as_mut() {
                served.record_frame(&report);
            }
            #[cfg(feature = "mcp")]
            if let Some(looked) = &looked {
                ljos_dev::top::add_work(&mut report, looked);
            }
            tools.top.record(view.engine(), &report);
        }
        if let Some(top) = tools.top.view() {
            paint_top(ui, &top);
        }
    }
}

/// What the inspector is told of drawing the primary window, which the
/// scene cannot see: the painter, what each pass cost, and the start-up.
#[cfg(feature = "mcp")]
impl<M> ShellApp<M> {
    /// Whether an inspector takes it; nothing is measured otherwise.
    pub(crate) fn inspected(&self) -> bool {
        self.tools.inspector.is_some()
    }

    pub(crate) fn set_painter(&mut self, painter: ljos_dev::mcp::PainterInfo) {
        if let Some(served) = self.tools.inspector.as_mut() {
            served.inspector_mut().set_painter(painter);
        }
    }

    pub(crate) fn record_startup(
        &mut self,
        step: ljos_dev::mcp::StartupStep,
        at: std::time::Duration,
    ) {
        if let Some(served) = self.tools.inspector.as_mut() {
            served.inspector_mut().record_startup(step, at);
        }
    }

    pub(crate) fn record_paint(&mut self, report: &ljos_dev::mcp::PaintReport) {
        if let Some(served) = self.tools.inspector.as_mut() {
            served.inspector_mut().record_paint(report);
        }
    }

    /// Whether a client follows items, so that each pass says where it
    /// changed the picture.
    pub(crate) fn watches_items(&self) -> bool {
        self.tools
            .inspector
            .as_ref()
            .is_some_and(|served| served.inspector().watches_items())
    }

    /// Where a pass of the primary window changed the picture; `None` for
    /// all of it.
    pub(crate) fn record_changed(
        &mut self,
        engine: &ljos_engine::Engine,
        areas: Option<&[[f64; 4]]>,
    ) {
        if let Some(served) = self.tools.inspector.as_mut() {
            served.inspector_mut().record_changed(engine, areas);
        }
    }

    /// Says that the primary window is the platform's, so that
    /// `app_resize` asks the platform for its size.
    pub(crate) fn set_resizes_window(&mut self) {
        if let Some(served) = self.tools.inspector.as_mut() {
            served.inspector_mut().set_resizes_window();
        }
    }

    /// The size an agent asked the primary window to have.
    pub(crate) fn take_resize_request(&mut self) -> Option<[f64; 2]> {
        self.tools
            .inspector
            .as_mut()
            .and_then(|served| served.inspector_mut().take_resize_request())
    }

    /// Whether an agent waits for the primary window to have a size.
    pub(crate) fn waits_for_resize(&self) -> bool {
        self.tools
            .inspector
            .as_ref()
            .is_some_and(|served| served.inspector().waits_for_resize())
    }

    /// Answers the agent that waits for a size, if the scene has it or
    /// the platform has `settled` on another.
    pub(crate) fn finish_resize(&mut self, settled: bool) {
        let Self { view, tools, .. } = self;
        if let Some(served) = tools.inspector.as_mut() {
            served.inspector_mut().finish_resize(view.scene(), settled);
        }
    }

    /// Says that the shell tells `app_memory` what each window's scene
    /// and renderer hold.
    pub(crate) fn set_measures_render(&mut self) {
        if let Some(served) = self.tools.inspector.as_mut() {
            served.inspector_mut().set_measures_render();
        }
    }

    /// Whether an `app_memory` waits to be told what the windows hold.
    pub(crate) fn waits_for_memory(&self) -> bool {
        self.tools
            .inspector
            .as_ref()
            .is_some_and(|served| served.inspector().waits_for_memory())
    }

    /// Answers the `app_memory` requests that wait.
    pub(crate) fn finish_memory(&mut self, windows: &[ljos_dev::mcp::WindowMemory]) {
        let Self { view, tools, .. } = self;
        if let Some(served) = tools.inspector.as_mut() {
            served.inspector_mut().finish_memory(view.engine(), windows);
        }
    }

    /// A frame of the primary window's display: what a slow frame is
    /// slower than.
    pub(crate) fn set_frame_budget(&mut self, budget: std::time::Duration) {
        if let Some(served) = self.tools.inspector.as_mut() {
            served.inspector_mut().set_frame_budget(budget);
        }
    }
}

/// The overlay: a sheet in the window's top right corner, in a font of
/// equal widths so the columns stand.
fn paint_top(ui: &mut egui::Ui, top: &TopView) {
    let window = ui.max_rect();
    egui::Area::new(egui::Id::new("ljos-top"))
        .order(egui::Order::Foreground)
        .fixed_pos(window.right_top())
        .pivot(egui::Align2::RIGHT_TOP)
        // Clicks go to the application underneath.
        .interactable(false)
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(SHEET)
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    for (i, line) in top.lines().iter().enumerate() {
                        let color = if i == 0 { TITLE } else { TEXT };
                        ui.add(
                            egui::Label::new(egui::RichText::new(line).monospace().color(color))
                                .extend()
                                .selectable(false),
                        );
                    }
                });
        });
}

const SHEET: egui::Color32 = egui::Color32::from_rgba_premultiplied(0x10, 0x14, 0x18, 0xe6);
const TITLE: egui::Color32 = egui::Color32::from_rgb(0x9c, 0xdc, 0xfe);
const TEXT: egui::Color32 = egui::Color32::from_rgb(0xe6, 0xe6, 0xe6);

#[cfg(feature = "mcp")]
pub use inspector::{InspectorAccess, InspectorAddress, InspectorOptions};

#[cfg(feature = "mcp")]
mod inspector {
    use std::net::SocketAddr;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::Arc;

    use ljos_dev::mcp::{
        self, Cap, CapSet, HttpConfig, HttpServer, Inspector, InspectorConfig, Listen,
    };
    use ljos_egui::QmlView;
    use ljos_engine::LogEntry;
    use ljos_runtime::FrameReport;

    use crate::{Setup, SetupError, ShellApp};

    /// How the inspector of a window is served.
    #[derive(Clone, Debug)]
    pub struct InspectorOptions {
        /// The loopback port; 0 lets the system pick one.
        pub port: u16,
        /// Where the address and the token of this start are written, for
        /// the owner alone to read. See
        /// [`ljos_dev::mcp::default_token_file`].
        pub token_file: PathBuf,
        /// Mask the value of every editable field, not only passwords.
        pub redact_all: bool,
    }

    impl InspectorOptions {
        /// A port the system picks, and the token file where clients look
        /// for it: `target/ljos/mcp-<pid>.json` below `dir`.
        pub fn below(dir: impl Into<PathBuf>) -> Self {
            Self {
                port: 0,
                token_file: mcp::default_token_file(dir),
                redact_all: false,
            }
        }
    }

    /// What clients of the inspector may do and where it listens, beyond
    /// the defaults: the capabilities `inspect` only, a loopback port.
    #[derive(Clone, Debug)]
    pub struct InspectorAccess {
        /// Granted to every client. `eval` is never implied by another.
        pub caps: CapSet,
        /// An address given on purpose (`--mcp-bind`) instead of the
        /// loopback port of the options.
        pub bind: Option<SocketAddr>,
        /// A Unix socket instead of a port; the owner alone can use it.
        pub socket: Option<PathBuf>,
        /// The second word an address beyond this machine needs.
        pub insecure_lan: bool,
        /// Remove the token file and the socket when SIGINT or SIGTERM
        /// ends the process. For an application's own process.
        pub clean_up_on_signals: bool,
    }

    impl Default for InspectorAccess {
        fn default() -> Self {
            Self {
                caps: CapSet::of([Cap::Inspect]),
                bind: None,
                socket: None,
                insecure_lan: false,
                clean_up_on_signals: false,
            }
        }
    }

    /// Where a served inspector is.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct InspectorAddress {
        /// `http://127.0.0.1:<port>/mcp`.
        pub url: String,
        /// The file with the URL and the bearer token a client shows.
        pub token_file: PathBuf,
        /// Said to the person who started it, for a server that listens
        /// beyond this machine; empty otherwise.
        pub warning: Option<String>,
    }

    /// The inspector of a window and its server. The server goes first:
    /// nothing is taken from clients that nobody will answer.
    pub(crate) struct Served {
        server: HttpServer,
        inspector: Inspector,
        address: InspectorAddress,
    }

    impl Served {
        pub(crate) fn pump(&mut self, view: &mut QmlView) {
            let engine = Rc::clone(view.engine());
            // Clients are told of a reload the watcher applied before this
            // step; it compares two numbers.
            self.inspector.observe(&engine);
            if !self.inspector.has_work() {
                return;
            }
            self.inspector.set_windows(window_count(view));
            let clock = Arc::clone(view.clock());
            self.inspector.pump(&engine, view.scene_mut(), &*clock);
        }

        /// Says whether the application's files are watched, which is
        /// what lets an agent ask for a reload.
        pub(crate) fn set_watched(&mut self, watched: bool) {
            self.inspector.set_watched(watched);
        }

        /// Whether an agent asked for a reload that nobody has started.
        pub(crate) fn take_reload_request(&mut self) -> bool {
            self.inspector.take_reload_request()
        }

        /// Answers the agent that waits for a reload, with how it went.
        pub(crate) fn finish_reload(&mut self, view: &QmlView) {
            self.inspector.finish_reload(view.engine());
        }

        pub(crate) fn record_frame(&mut self, report: &FrameReport) {
            self.inspector.record_frame(report);
        }

        pub(crate) fn record_scene_frames(
            &mut self,
            scene: &mut ljos_quick::QuickScene,
            drained: usize,
        ) {
            self.inspector.record_scene_frames(scene, drained);
        }

        pub(crate) fn take_work(&mut self) -> Option<FrameReport> {
            self.inspector.take_work()
        }

        pub(crate) fn inspector(&self) -> &Inspector {
            &self.inspector
        }

        /// What the host says of painting the window.
        pub(crate) fn inspector_mut(&mut self) -> &mut Inspector {
            &mut self.inspector
        }
    }

    /// How many windows the user sees. The inspector looks at the primary
    /// one only; it says how many there are so that an agent does not take
    /// what it sees for all of the application.
    pub(super) fn window_count(view: &QmlView) -> usize {
        let engine = view.engine();
        let sys = ljos_controls::WindowSystem::of(engine);
        if !sys.is_hosted() {
            // A `Window` is an item of the one scene.
            return 1;
        }
        let shown = sys
            .windows(engine)
            .into_iter()
            .filter(|w| sys.spec(engine, *w).is_some_and(|s| s.visible))
            .count();
        // The shell makes a window for a root that is no `Window`.
        let root_is_window = view
            .scene()
            .root()
            .is_some_and(|r| sys.spec(engine, r).is_some());
        shown + usize::from(!root_is_window)
    }

    impl<M> Setup<'_, M> {
        /// Serves the read-only inspector of this window to MCP clients
        /// on this machine (ARCHITECTURE.md §19.4, §19.7): a loopback
        /// port, and a token made for this start in a file only its owner
        /// can read. The server stops and the file is removed when the
        /// window closes.
        ///
        /// The inspector reads the engine's log, and an engine has one
        /// log handler: what the host did with a line (print it) goes in
        /// `on_log`, and a handler set before this call is replaced.
        ///
        /// # Errors
        /// The port is taken, or the token file cannot be written.
        pub fn serve_inspector(
            &mut self,
            options: InspectorOptions,
            on_log: impl FnMut(&LogEntry) + 'static,
        ) -> Result<InspectorAddress, SetupError> {
            self.serve_inspector_as(options, InspectorAccess::default(), on_log)
        }

        /// As [`serve_inspector`](Self::serve_inspector), with the
        /// capabilities clients hold and the transport given.
        ///
        /// # Errors
        /// As there, and an address beyond this machine without
        /// `insecure_lan`.
        pub fn serve_inspector_as(
            &mut self,
            options: InspectorOptions,
            access: InspectorAccess,
            on_log: impl FnMut(&LogEntry) + 'static,
        ) -> Result<InspectorAddress, SetupError> {
            let cfg = InspectorConfig {
                redact_all: options.redact_all,
                background: self.tools.background,
                ..InspectorConfig::default()
            };
            let (inspector, surface) = Inspector::new(cfg, Arc::clone(&self.tools.wake));
            inspector.watch_log(self.engine, on_log);
            let mut config = HttpConfig::loopback(access.caps);
            config.listen = match (access.socket, access.bind) {
                (Some(path), _) => Listen::Unix(path),
                (None, Some(addr)) => Listen::Address(addr),
                (None, None) => Listen::Loopback(options.port),
            };
            config.insecure_lan = access.insecure_lan;
            config.clean_up_on_signals = access.clean_up_on_signals;
            let server = mcp::serve_with(surface, config, &options.token_file)?;
            let address = InspectorAddress {
                url: server.url().to_owned(),
                token_file: options.token_file,
                warning: server.warning().map(str::to_owned),
            };
            self.tools.inspector = Some(Served {
                server,
                inspector,
                address: address.clone(),
            });
            Ok(address)
        }
    }

    impl<M> ShellApp<M> {
        /// Where this window's inspector is served; `None` without one.
        pub fn inspector_address(&self) -> Option<&InspectorAddress> {
            self.tools.inspector.as_ref().map(|s| &s.address)
        }

        /// Stops the inspector's server and removes its token file. The
        /// window closing does the same.
        pub fn stop_inspector(&mut self) {
            if let Some(mut served) = self.tools.inspector.take() {
                served.server.stop();
            }
        }
    }
}

#[cfg(all(test, feature = "mcp"))]
mod tests {
    use std::sync::Arc;

    use ljos_runtime::SystemClock;

    use crate::{Hosting, ShellApp, ShellConfig};

    fn count_in(qml: &str, hosted: bool) -> usize {
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().canonicalize().unwrap().join("Main.qml");
        std::fs::write(&entry, qml).unwrap();
        let ctx = egui::Context::default();
        let setup = |setup: &mut crate::Setup<'_, ()>| setup.watch(&entry);
        let app = if hosted {
            let hosting = Hosting {
                tree: egui::accesskit::TreeId::ROOT,
                changed: Box::new(|| {}),
                wake: Arc::new(|_| {}),
            };
            ShellApp::hosted(
                &ctx,
                &ShellConfig::default(),
                Arc::new(SystemClock::new()),
                setup,
                hosting,
            )
        } else {
            ShellApp::new(
                &ctx,
                &ShellConfig::default(),
                Arc::new(SystemClock::new()),
                setup,
            )
        }
        .unwrap();
        super::inspector::window_count(app.view())
    }

    const WINDOWS: &str = "import QtQuick\nimport QtQuick.Window\nItem {\n  Window { visible: true; width: 100; height: 100 }\n  Window { visible: true; width: 50; height: 50 }\n  Window { visible: false }\n}\n";

    #[test]
    fn the_windows_the_user_sees_are_counted_and_the_shell_one_for_a_plain_root() {
        // Two shown, one hidden, and the window the shell makes for the
        // item that is the root.
        assert_eq!(count_in(WINDOWS, true), 3);
    }

    #[test]
    fn a_root_that_is_a_window_is_one_of_them() {
        let qml = "import QtQuick\nimport QtQuick.Window\nWindow { visible: true; width: 10; height: 10 }\n";
        assert_eq!(count_in(qml, true), 1);
    }

    #[test]
    fn without_a_host_for_windows_there_is_one() {
        assert_eq!(count_in(WINDOWS, false), 1);
    }
}
