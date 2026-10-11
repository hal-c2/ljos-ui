//! Scenario state. One [`LjosWorld`] per scenario; `!Send` is fine.

use std::collections::BTreeMap;

use cucumber::World;

use crate::backend::{Backend, BackendError, BackendMode, QmlFile};

/// File the `Given the QML:` doc string is written to.
pub const MAIN_FILE: &str = "Scenario.qml";

#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct LjosWorld {
    mode: BackendMode,
    backends: Vec<Box<dyn Backend>>,
    files: BTreeMap<String, String>,
}

impl LjosWorld {
    fn new() -> Self {
        Self {
            mode: BackendMode::from_env(),
            backends: Vec::new(),
            files: BTreeMap::new(),
        }
    }

    /// Called from the `before` hook: picks the backends this scenario runs on.
    pub fn prepare(&mut self, qt_parity: bool) {
        self.backends = self.mode.backends_for(qt_parity);
    }

    /// Called from the `before` hook for a scenario that changes files
    /// while the application runs: the backends note what a reload needs
    /// from the first load on.
    pub fn watch_files(&mut self) {
        for b in &mut self.backends {
            b.watch_files();
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
        self.each(|b| b.change_file(name, source))
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

    fn files_for(&self, entry: &str) -> Result<Vec<QmlFile>, String> {
        if !self.files.contains_key(entry) {
            return Err(format!(
                "no file named {entry:?}; add it with `Given the file {entry:?}:`"
            ));
        }
        Ok(self
            .files
            .iter()
            .map(|(name, source)| QmlFile {
                name: name.clone(),
                source: source.clone(),
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
