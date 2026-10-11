//! Development tools in the shell (feature `dev`): the `top` overlay
//! (ARCHITECTURE.md §3.4, §19.10) and, with the feature `mcp`, the
//! read-only inspector an agent connects to (§19.4).
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
            let Self { view, tools, .. } = self;
            if let Some(served) = tools.inspector.as_mut() {
                view.without_wake(|view| served.pump(view));
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
pub use inspector::{InspectorAddress, InspectorOptions};

#[cfg(feature = "mcp")]
mod inspector {
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::Arc;

    use ljos_dev::mcp::{self, HttpServer, Inspector, InspectorConfig, Listen};
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

    /// Where a served inspector is.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct InspectorAddress {
        /// `http://127.0.0.1:<port>/mcp`.
        pub url: String,
        /// The file with the URL and the bearer token a client shows.
        pub token_file: PathBuf,
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
            if !self.inspector.has_work() {
                return;
            }
            let engine = Rc::clone(view.engine());
            let clock = Arc::clone(view.clock());
            self.inspector.pump(&engine, view.scene_mut(), &*clock);
        }

        pub(crate) fn record_frame(&mut self, report: &FrameReport) {
            self.inspector.record_frame(report);
        }

        pub(crate) fn take_work(&mut self) -> Option<FrameReport> {
            self.inspector.take_work()
        }
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
            let cfg = InspectorConfig {
                redact_all: options.redact_all,
                background: self.tools.background,
                ..InspectorConfig::default()
            };
            let (inspector, surface) = Inspector::new(cfg, Arc::clone(&self.tools.wake));
            inspector.watch_log(self.engine, on_log);
            let server = mcp::serve(surface, Listen::Loopback(options.port), &options.token_file)?;
            let address = InspectorAddress {
                url: server.url().to_owned(),
                token_file: options.token_file,
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
