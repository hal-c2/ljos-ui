//! A shipped application its users change while it runs (feature
//! `user-qml`, ARCHITECTURE.md §12.1): the user's files over the
//! application's, each save applied between two frames with the state of
//! the application kept. Nothing is drawn for it: a fault is the
//! application's to tell (`UserQml::on_fault`), never an overlay.
//!
//! The compiler is linked, and run only once the user has a file.

use std::rc::Rc;
use std::sync::Arc;

use ljos_dev::{Host, Reloader, Report, TypeOracle, UserQml, UserSession};
use ljos_egui::QmlView;
use ljos_engine::{Engine, MapSource, ObjectId};
use ljos_runtime::WakeHook;

use crate::{Setup, ShellApp};

type ReportHandler = Box<dyn FnMut(&Report)>;

/// What `Setup` holds for the feature.
pub(crate) struct UserSetup {
    wake: WakeHook,
    session: Option<UserSession>,
    on_report: Option<ReportHandler>,
}

impl UserSetup {
    pub(crate) fn new(wake: WakeHook) -> Self {
        Self {
            wake,
            session: None,
            on_report: None,
        }
    }

    /// The session, told of the root the setup made.
    pub(crate) fn finish(self, root: ObjectId) -> Option<User> {
        let mut session = self.session?;
        session.set_root(root);
        Some(User {
            session,
            on_report: self.on_report,
        })
    }
}

/// The user's files in a [`ShellApp`].
pub(crate) struct User {
    session: UserSession,
    on_report: Option<ReportHandler>,
}

impl<M> Setup<'_, M> {
    /// Gives the engine the application's compiled files `pack` with the
    /// files of its user's directory over them: the root made after this
    /// is made of the user's file of its key when there is one. `oracle`
    /// knows the types the program registers, so call it once they are.
    ///
    /// The pack must carry its sources (`build::Options::with_sources`)
    /// for a user's file to compile; one without runs as built, and says
    /// why through [`UserQml::on_fault`](ljos_dev::UserQml::on_fault).
    pub fn user_qml<O>(&mut self, pack: MapSource, config: UserQml, oracle: O)
    where
        O: TypeOracle + Clone + Send + 'static,
        Reloader<O>: Send,
    {
        let session = UserSession::start(
            self.engine,
            pack,
            config,
            oracle,
            Arc::clone(&self.user.wake),
        );
        self.user.session = Some(session);
    }

    /// Runs on the UI thread for what the user's files brought: a save
    /// that was applied, one that failed, a fault that went away.
    pub fn on_user_report(&mut self, f: impl FnMut(&Report) + 'static) {
        self.user.on_report = Some(Box::new(f));
    }
}

/// Where a reload of the user's files shows a new root.
struct ViewHost<'a> {
    view: &'a mut QmlView,
}

impl Host for ViewHost<'_> {
    fn window(&self) -> Option<ObjectId> {
        self.view.scene().root()
    }

    fn show(&mut self, engine: &Engine, old: Option<ObjectId>, new: ObjectId) {
        crate::roots::show_root(self.view, engine, old, new);
    }
}

impl<M> ShellApp<M> {
    /// How many saves of the user's files were applied; None without a
    /// user's directory.
    pub fn user_reloads(&self) -> Option<u64> {
        self.user.as_ref().map(|u| u.session.reloads())
    }

    /// Whether the user's files are compiled and run (not only the
    /// application's).
    pub fn user_files_active(&self) -> bool {
        self.user.as_ref().is_some_and(|u| u.session.is_active())
    }

    pub(crate) fn pump_user(&mut self) {
        let Self { view, user, .. } = self;
        let Some(User { session, on_report }) = user else {
            return;
        };
        let reports = view.without_wake(|view| {
            let engine = Rc::clone(view.engine());
            let clock = Arc::clone(view.clock());
            let mut host = ViewHost { view };
            session.pump(&engine, &*clock, &mut host)
        });
        if let Some(f) = on_report.as_mut() {
            for report in &reports {
                f(report);
            }
        }
    }
}
