//! Dev hot reload in the shell (feature `dev`, ARCHITECTURE.md §12.1):
//! the QML files are compiled when the window opens, watched while it is
//! open, and every save is applied between two frames with the state of
//! the application kept. A change that cannot be applied leaves the
//! application as it was and is shown in an overlay until it is mended.
//!
//! This is the one place where the shell links the compiler, and so the
//! parser. A shipped application builds without the feature and links
//! neither (`tests/no_parser.rs`).

use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use ljos_dev::{DevSession, Host, Overlay, Report};
use ljos_egui::QmlView;
use ljos_engine::{Engine, LogLevel, ObjectId};
use ljos_quick::Item;
use ljos_runtime::WakeHook;

use crate::{Setup, SetupError, ShellApp};

type ReportHandler = Box<dyn FnMut(&Report)>;

/// What `Setup` holds for the feature: how the watcher wakes the window,
/// and what [`Setup::watch`] made.
pub(crate) struct DevSetup {
    wake: WakeHook,
    dev: Option<Dev>,
    on_report: Option<ReportHandler>,
}

impl DevSetup {
    pub(crate) fn new(ctx: &egui::Context) -> Self {
        let ctx = ctx.clone();
        Self {
            wake: Arc::new(move || ctx.request_repaint()),
            dev: None,
            on_report: None,
        }
    }

    /// The watched application, when the setup closure asked for one.
    pub(crate) fn finish(self) -> Option<Dev> {
        let mut dev = self.dev?;
        dev.on_report = self.on_report;
        Some(dev)
    }
}

/// A watched application in a [`ShellApp`].
pub(crate) struct Dev {
    session: DevSession,
    /// Shown while the project has never compiled, so that there is a
    /// window to show why.
    placeholder: Option<ObjectId>,
    on_report: Option<ReportHandler>,
}

impl<M> Setup<'_, M> {
    /// Compiles the directory of `main` (the entry file, such as
    /// `ui/Main.qml`), makes its root and watches the directory: from
    /// here on a save is applied to the running application. Returns the
    /// root for the setup closure to return.
    ///
    /// A project that does not compile opens an empty window with the
    /// error overlay; the first save that compiles fills it.
    ///
    /// # Errors
    /// `main` is not a `.qml` file that can be read, or its directory
    /// cannot be watched.
    pub fn watch(&mut self, main: &Path) -> Result<ObjectId, SetupError> {
        let engine = self.engine;
        let session = DevSession::open(engine, main, Arc::clone(&self.dev.wake))?;
        let (root, placeholder) = match session.root() {
            Some(root) => (root, None),
            None => {
                let empty = engine
                    .create_native(Item::default(), None)
                    .map_err(|e| e.to_string())?;
                engine.complete(empty).map_err(|e| e.to_string())?;
                (empty, Some(empty))
            }
        };
        self.dev.dev = Some(Dev {
            session,
            placeholder,
            on_report: None,
        });
        Ok(root)
    }

    /// Runs on the UI thread for everything the watcher brought: a reload
    /// that was applied (with how long it took), one that failed, an
    /// error that went away. For a host that tells the developer.
    pub fn on_reload(&mut self, f: impl FnMut(&Report) + 'static) {
        self.dev.on_report = Some(Box::new(f));
    }
}

/// The view as a reload sees it: where the keyboard focus is looked up,
/// and where a new root is shown.
struct ViewHost<'a> {
    view: &'a mut QmlView,
    placeholder: &'a mut Option<ObjectId>,
}

impl Host for ViewHost<'_> {
    fn window(&self) -> Option<ObjectId> {
        self.view.scene().root()
    }

    fn show(&mut self, engine: &Engine, old: Option<ObjectId>, new: ObjectId) {
        let placeholder = self.placeholder.take();
        // The view sizes its root when the window's size changes, which
        // it has not: the new root takes the size the old one was given.
        if let Some(shown) = old.or(placeholder) {
            for size in [Item::WIDTH, Item::HEIGHT] {
                if let Ok(v) = engine.get(shown, size) {
                    let _ = engine.set(new, size, v);
                }
            }
        }
        if let Err(e) = self.view.scene_mut().set_root(engine, new) {
            engine.log(
                LogLevel::Warning,
                "ljos.reload",
                format!("the new root cannot be shown: {e}"),
            );
        }
        if let Some(empty) = placeholder {
            engine.destroy(empty);
        }
    }
}

impl<M> ShellApp<M> {
    /// How many reloads were applied since the window opened; None when
    /// the application is not watched.
    pub fn reloads(&self) -> Option<u64> {
        self.dev.as_ref().map(|d| d.session.reloads())
    }

    /// What the error overlay says; None when it is not shown.
    pub fn reload_overlay(&self) -> Option<Overlay> {
        let dev = self.dev.as_ref()?;
        Overlay::current(self.view.engine(), dev.session.more_errors())
    }

    /// Pipeline step 1, after the inbox: applies what the watcher brought.
    /// The view's waker is held, as for the inbox: the pass that follows
    /// shows the result.
    pub(crate) fn pump_dev(&mut self) {
        let Self { view, dev, .. } = self;
        let Some(dev) = dev else {
            return;
        };
        let Dev {
            session,
            placeholder,
            on_report,
        } = dev;
        let reports = view.without_wake(|view| {
            let engine = Rc::clone(view.engine());
            let clock = Arc::clone(view.clock());
            let mut host = ViewHost { view, placeholder };
            session.pump(&engine, &*clock, &mut host)
        });
        if let Some(f) = on_report.as_mut() {
            for report in &reports {
                f(report);
            }
        }
    }

    /// Paints the error overlay over the application: a sheet along the
    /// bottom of the window, so that the application, which runs on as it
    /// was, stays in view and in use.
    pub(crate) fn show_dev(&self, ui: &mut egui::Ui) {
        let Some(overlay) = self.reload_overlay() else {
            return;
        };
        let window = ui.max_rect();
        egui::Area::new(egui::Id::new("ljos-reload-error"))
            .order(egui::Order::Foreground)
            .fixed_pos(window.left_bottom())
            .pivot(egui::Align2::LEFT_BOTTOM)
            // Clicks go to the application underneath.
            .interactable(false)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(SHEET)
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(window.width() - 24.0);
                        let line = |text: &str, color, strong: bool| {
                            let text = egui::RichText::new(text).color(color);
                            egui::Label::new(if strong { text.strong() } else { text })
                                .wrap()
                                .selectable(false)
                        };
                        ui.add(line(&overlay.title, TITLE, true));
                        if !overlay.location.is_empty() {
                            ui.add(line(&overlay.location, PLACE, false));
                        }
                        ui.add(line(&overlay.message, TEXT, false));
                        ui.add(line(&overlay.hint, HINT, false));
                    });
            });
    }
}

const SHEET: egui::Color32 = egui::Color32::from_rgb(0x2b, 0x12, 0x14);
const TITLE: egui::Color32 = egui::Color32::from_rgb(0xff, 0x8a, 0x80);
const PLACE: egui::Color32 = egui::Color32::from_rgb(0xff, 0xd1, 0x80);
const TEXT: egui::Color32 = egui::Color32::WHITE;
const HINT: egui::Color32 = egui::Color32::from_rgb(0xb0, 0xb0, 0xb0);
