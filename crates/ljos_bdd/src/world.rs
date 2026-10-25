//! Scenario state. One [`LjosWorld`] per scenario; `!Send` is fine.

use std::collections::BTreeMap;

use cucumber::World;

use crate::backend::testrun::{TestReport, TestRun};
use crate::backend::{Backend, BackendError, BackendMode, QmlFile};

/// File the `Given the QML:` doc string is written to.
pub const MAIN_FILE: &str = "Scenario.qml";

#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct LjosWorld {
    mode: BackendMode,
    backends: Vec<Box<dyn Backend>>,
    files: BTreeMap<String, String>,
    /// What each backend's test runner said, for the scenarios that run
    /// `tst_*.qml` files.
    test_reports: Vec<(&'static str, TestReport)>,
    /// The authoring tools and the command line, for the scenarios that
    /// ask them and no running application.
    author: crate::author::Slot,
    /// The scenario's web server, started by the first step that serves a
    /// file or by QML that names it.
    web: Option<crate::web::WebServer>,
}

impl LjosWorld {
    fn new() -> Self {
        Self {
            mode: BackendMode::from_env(),
            backends: Vec::new(),
            files: BTreeMap::new(),
            test_reports: Vec::new(),
            author: crate::author::Slot::default(),
            web: None,
        }
    }

    /// The authoring side of the scenario, with the files staged so far:
    /// it writes them into its project before it is asked anything.
    pub fn author(&mut self) -> (&mut crate::author::Slot, &BTreeMap<String, String>) {
        (&mut self.author, &self.files)
    }

    /// Called from the `before` hook: picks the backends this scenario runs on.
    pub fn prepare(&mut self, qt_parity: bool, desktop: bool) {
        self.backends = self.mode.backends_for(qt_parity, desktop);
    }

    /// Called from the `before` hook for a scenario that changes files
    /// while the application runs: the backends note what a reload needs
    /// from the first load on.
    pub fn watch_files(&mut self) {
        for b in &mut self.backends {
            if let Some(reload) = b.as_reload() {
                reload.watch_files();
            }
        }
    }

    /// A file of the running application is saved with new text, or
    /// removed (`None`). A later `load` sees the files as they are now.
    ///
    /// # Errors
    /// A backend cannot change files under a running application.
    pub fn change_file(&mut self, name: &str, source: Option<&str>) -> Result<(), String> {
        match source {
            Some(text) => self.files.insert(name.to_owned(), text.to_owned()),
            None => self.files.remove(name),
        };
        self.each(|b| b.reloading()?.change_file(name, source))
    }

    /// As [`change_file`](Self::change_file), by an editor that closes
    /// the file it wrote: the save is whole.
    ///
    /// # Errors
    /// A backend cannot change files under a running application.
    pub fn save_file_whole(&mut self, name: &str, source: &str) -> Result<(), String> {
        self.change_file(name, Some(source))?;
        self.each(|b| b.reloading()?.finish_save(name))
    }

    /// A file of the running application is emptied by a save that has not
    /// finished.
    ///
    /// # Errors
    /// A backend cannot change files under a running application.
    pub fn empty_file(&mut self, name: &str) -> Result<(), String> {
        self.files.insert(name.to_owned(), String::new());
        self.each(|b| b.reloading()?.empty_file(name))
    }

    /// The scenario's web server, started if it is not yet.
    ///
    /// # Errors
    /// It cannot listen.
    pub fn web(&mut self) -> Result<&crate::web::WebServer, String> {
        if self.web.is_none() {
            self.web = Some(crate::web::WebServer::start()?);
        }
        self.web
            .as_ref()
            .ok_or_else(|| "the scenario's web server did not start".to_owned())
    }

    /// Stage a source file for the next `load`.
    pub fn stage(&mut self, name: &str, source: &str) {
        self.files.insert(name.to_owned(), source.to_owned());
    }

    /// Instantiate `entry` (staged earlier) on every backend.
    ///
    /// # Errors
    /// The entry is unknown or a backend fails.
    pub fn load(&mut self, entry: &str) -> Result<(), String> {
        let files = self.files_for(entry)?;
        self.each(|b| b.load(&files, entry))
    }

    /// Like [`Self::load`], but the load must fail with an error of the application's own
    /// (a "not implemented yet" backend does not count as rejecting).
    ///
    /// # Errors
    /// The entry is unknown, or some backend accepted it or could not answer.
    pub fn load_rejected(&mut self, entry: &str) -> Result<(), String> {
        let files = self.files_for(entry)?;
        self.each(|b| match b.load(&files, entry) {
            Err(BackendError::Failed(_)) => Ok(()),
            Err(other) => Err(other),
            Ok(()) => Err(BackendError::Failed(format!(
                "{entry:?} loaded without an error"
            ))),
        })
    }

    /// Run every staged file with each backend's test runner and keep
    /// what the runners said.
    ///
    /// # Errors
    /// A runner could not be run. A failing test is not an error.
    pub fn run_tests(&mut self, run: &TestRun) -> Result<(), String> {
        let files = self.staged()?;
        let mut reports = Vec::new();
        let ran = self.each(|b| {
            reports.push((b.name(), b.tooling()?.run_tests(&files, run)?));
            Ok(())
        });
        self.test_reports = reports;
        ran
    }

    /// Check what each runner said; the failures are labelled with the
    /// backend, as [`Self::each`] labels them.
    ///
    /// # Errors
    /// No tests were run, or one line per backend whose report fails `f`.
    pub fn each_test_report(
        &self,
        f: impl Fn(&TestReport) -> Result<(), String>,
    ) -> Result<(), String> {
        if self.test_reports.is_empty() {
            return Err("no tests were run; use `When the tests are run`".into());
        }
        let failures: Vec<String> = self
            .test_reports
            .iter()
            .filter_map(|(name, report)| f(report).err().map(|e| format!("[{name}] FAILED: {e}")))
            .collect();
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n"))
        }
    }

    fn files_for(&mut self, entry: &str) -> Result<Vec<QmlFile>, String> {
        if !self.files.contains_key(entry) {
            return Err(format!(
                "no file named {entry:?}; add it with `Given the file {entry:?}:`"
            ));
        }
        self.staged()
    }

    /// Every staged file, with `$SERVER` the scenario's web server.
    fn staged(&mut self) -> Result<Vec<QmlFile>, String> {
        let names_server = self
            .files
            .values()
            .any(|s| s.contains(crate::web::PLACEHOLDER));
        let address = if names_server {
            Some(self.web()?.address())
        } else {
            None
        };
        Ok(self
            .files
            .iter()
            .map(|(name, source)| QmlFile {
                name: name.clone(),
                source: match &address {
                    Some(a) => source.replace(crate::web::PLACEHOLDER, a),
                    None => source.clone(),
                },
            })
            .collect())
    }

    /// Run `f` on every backend of the scenario and gather the failures, each
    /// labelled with the backend that produced it. A scenario passes only if
    /// every backend passes.
    ///
    /// # Errors
    /// One line per failing backend.
    pub fn each(
        &mut self,
        mut f: impl FnMut(&mut dyn Backend) -> Result<(), BackendError>,
    ) -> Result<(), String> {
        if self.backends.is_empty() {
            return Err("no backend selected for this scenario (see LJOS_BDD_BACKEND)".into());
        }
        let mut failures = Vec::new();
        for b in &mut self.backends {
            if let Err(e) = f(b.as_mut()) {
                let tag = if matches!(e, BackendError::NotImplemented(_)) {
                    "PENDING"
                } else {
                    "FAILED"
                };
                failures.push(format!("[{}] {tag}: {e}", b.name()));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n"))
        }
    }
}
