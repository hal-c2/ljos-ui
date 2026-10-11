//! The World an external suite's step adapter works on.
//!
//! An adapter crate (`acceptance/<suite>/steps`) writes `#[given]`,
//! `#[when]` and `#[then]` steps in the suite's own words and maps each onto
//! this World. The World owns one Ljós backend; the adapter never sees egui
//! or the engine, only [`Backend`], the same surface the Ljós ledger drives.

use std::collections::BTreeMap;

use cucumber::World;

use crate::backend::{Backend, QmlFile, ljos::LjosBackend};

#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct AcceptanceWorld {
    backend: LjosBackend,
    files: BTreeMap<String, String>,
}

impl AcceptanceWorld {
    fn new() -> Self {
        Self {
            backend: LjosBackend::new(),
            files: BTreeMap::new(),
        }
    }

    /// The application under test.
    pub fn backend(&mut self) -> &mut dyn Backend {
        &mut self.backend
    }

    /// Adds a QML file to what the next [`Self::load`] sees.
    pub fn stage(&mut self, name: &str, source: &str) {
        self.files.insert(name.to_owned(), source.to_owned());
    }

    /// Starts the application from the staged file `entry`.
    ///
    /// # Errors
    /// The entry was never staged, or it does not load.
    pub fn load(&mut self, entry: &str) -> Result<(), String> {
        if !self.files.contains_key(entry) {
            return Err(format!("no staged file named {entry:?}"));
        }
        let files: Vec<QmlFile> = self
            .files
            .iter()
            .map(|(name, source)| QmlFile {
                name: name.clone(),
                source: source.clone(),
            })
            .collect();
        self.backend.load(&files, entry).map_err(|e| e.to_string())
    }
}
