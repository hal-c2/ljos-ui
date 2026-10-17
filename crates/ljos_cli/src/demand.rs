//! The files of a run, compiled as the program reaches them.
//!
//! Qt loads the entry file and what it reaches: the types of its own
//! directory it uses, the directories and modules it imports, and at run
//! time what a `Loader` or `Qt.createComponent` is given. A file nothing
//! reaches is never compiled, so it says nothing and costs nothing. The
//! engine asks its source for a file exactly when it first makes an object
//! of it, so a source that compiles a file when it is asked for is that
//! walk, with no second one over the compiled files. Files are read the
//! same way: a directory when a file reached is in it or imports it
//! ([`Project::reach`]).

use std::{cell::RefCell, collections::HashSet, path::Path, sync::Arc};

use anyhow::Context as _;
use ljos_compiler::{Compiled, Project, Severity, compile_file, failure_reason};
use ljos_engine::{ComponentSource, MapSource};
use ljos_ir::CompiledFile;
use qml_front::TypeOracle;

/// What a file the program reached has to say: one line per warning.
pub(crate) type Say = Box<dyn Fn(&str)>;

/// A [`ComponentSource`] over a project's files that compiles each when the
/// engine first asks for it, and tells `say` its warnings then.
pub(crate) struct OnDemand<O> {
    /// What was reached of the tree, read as it is.
    project: RefCell<Project<O>>,
    /// What was compiled, or why it did not compile, with the URLs.
    done: RefCell<MapSource>,
    asked: RefCell<HashSet<String>>,
    say: Say,
}

impl<O: TypeOracle> OnDemand<O> {
    /// The project under `dir` with its entry `entry` compiled: an entry
    /// that is not there or does not compile stops the run before a window
    /// opens, as anything else stops only what makes an object of it.
    pub(crate) fn open(dir: &Path, entry: &str, oracle: O, say: Say) -> anyhow::Result<Self> {
        let mut project =
            Project::open(dir, oracle).with_context(|| format!("{}", dir.display()))?;
        project.reach(entry);
        if project.source(entry).is_none() {
            anyhow::bail!("{entry}.qml is not in {}", dir.display());
        }
        let base = format!("file://{}", dir.to_string_lossy().replace('\\', "/"));
        let source = Self {
            project: RefCell::new(project),
            done: RefCell::new(MapSource::new().with_base_url(base)),
            asked: RefCell::new(HashSet::new()),
            say,
        };
        source
            .reach(entry)
            .map_err(|first| anyhow::anyhow!("{entry}.qml:{first}"))?;
        Ok(source)
    }

    /// Compiles `key` the first time it is asked for. `Err` with its
    /// first error, in the compiler's words, when it did not compile then.
    fn reach(&self, key: &str) -> Result<(), String> {
        if !self.asked.borrow_mut().insert(key.to_owned()) {
            return Ok(());
        }
        // What another file reaches only when the program runs: a
        // `Loader` or `Qt.createComponent` of its URL.
        self.project.borrow_mut().reach(key);
        let project = self.project.borrow();
        if project.source(key).is_none() {
            return Ok(());
        }
        let result = compile_file(&project, key);
        let mut done = self.done.borrow_mut();
        match result {
            Ok(c) if !c.has_errors() => {
                for d in &c.diagnostics {
                    (self.say)(&format!("{key}.qml: {d}"));
                }
                done.insert(key, c.file);
                Ok(())
            }
            Ok(Compiled { diagnostics, .. }) | Err(diagnostics) => {
                // The loader's words for whatever makes an object of it.
                done.insert_failure(key, failure_reason(&diagnostics));
                Err(diagnostics
                    .iter()
                    .find(|d| d.severity == Severity::Error)
                    .map_or_else(|| "does not compile".to_owned(), ToString::to_string))
            }
        }
    }
}

impl<O: TypeOracle> OnDemand<O> {
    /// The files the project's builds keep inside the program
    /// ([`Project::resources`]).
    pub(crate) fn resources(&self) -> Vec<(String, std::path::PathBuf)> {
        self.project.borrow().resources().to_vec()
    }
}

#[cfg(test)]
impl<O: TypeOracle> OnDemand<O> {
    /// The files read so far.
    pub(crate) fn read(&self) -> Vec<String> {
        self.project.borrow().files().map(str::to_owned).collect()
    }
}

impl<O> std::fmt::Debug for OnDemand<O> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnDemand")
            .field("done", &self.done)
            .finish_non_exhaustive()
    }
}

impl<O: TypeOracle> ComponentSource for OnDemand<O> {
    fn load(&self, key: &str) -> Option<Arc<CompiledFile>> {
        let _ = self.reach(key);
        self.done.borrow().load(key)
    }

    fn failure(&self, key: &str) -> Option<String> {
        let _ = self.reach(key);
        self.done.borrow().failure(key)
    }

    fn url(&self, key: &str) -> String {
        self.done.borrow().url(key)
    }

    fn key(&self, url: &str) -> String {
        self.done.borrow().key(url)
    }
}
