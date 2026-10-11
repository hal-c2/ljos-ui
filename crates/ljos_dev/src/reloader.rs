//! From saved files to a patch: what the watcher's thread does once the
//! quiet time has passed, without a thread or a clock of its own, so a
//! test (and the scenario runner, where time is virtual) drives the same
//! code.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use ljos_compiler::{CompileFault, FileSet, ReloadSession};
use ljos_ir::ReloadPatch;
use qml_front::TypeOracle;

use crate::fingerprint::Fingerprints;

/// What a build made of the saves since the last one.
#[derive(Debug, Clone, PartialEq)]
pub enum Build {
    /// No file says anything else than at the last build: nothing was
    /// compiled, and whatever error is showing still stands.
    NoChange,
    /// The files compile and say what the application already runs (an
    /// edit was undone): there is nothing to apply and nothing wrong.
    UpToDate,
    /// The application has to take this. [`Reloader::ack`] or
    /// [`Reloader::reject`] comes before the next build.
    Patch {
        patch: Box<ReloadPatch>,
        /// What the compiler could not settle, and the reload lint.
        warnings: Vec<CompileFault>,
    },
    /// A file does not compile: nothing of the change is offered.
    Failed(Vec<CompileFault>),
}

/// A project's files as they change, and the patches that follow.
#[derive(Debug)]
pub struct Reloader<O> {
    session: ReloadSession<O>,
    prints: Fingerprints,
    /// Saves given as text, by key; None is a file that is gone.
    staged: BTreeMap<String, Option<String>>,
    /// Paths something happened to, read when the build starts: an editor
    /// is still writing when the first event comes.
    touched: BTreeSet<PathBuf>,
    rescan: bool,
    /// A `qmldir` or `CMakeLists.txt` changed: what a module has may
    /// differ although no QML file does.
    force: bool,
}

impl<O: TypeOracle + Clone> Reloader<O> {
    /// Over files given by [`save`](Self::save).
    pub fn in_memory(oracle: O) -> Self {
        Self::over(ReloadSession::in_memory(oracle))
    }

    /// Over every QML file under `root`.
    pub fn open(root: &Path, oracle: O) -> Result<Self, ljos_compiler::Error> {
        Ok(Self::over(ReloadSession::open(root, oracle)?))
    }

    fn over(session: ReloadSession<O>) -> Self {
        Self {
            session,
            prints: Fingerprints::new(),
            staged: BTreeMap::new(),
            touched: BTreeSet::new(),
            rescan: false,
            force: false,
        }
    }

    pub fn session(&self) -> &ReloadSession<O> {
        &self.session
    }

    /// The directory of a reloader made by [`open`](Self::open).
    pub fn root(&self) -> Option<&Path> {
        self.session.root()
    }

    /// Whether the next build reads the directory again: there may be
    /// directories to watch that were not there before.
    pub fn rescan_pending(&self) -> bool {
        self.rescan
    }

    /// Compiles everything, with every save so far, as what the
    /// application starts from. On `Err` the application has no files
    /// yet; the first build that compiles offers all of them as a patch.
    pub fn start(&mut self) -> Result<FileSet, Vec<CompileFault>> {
        self.take_saves();
        let files = self.session.start().cloned();
        self.prints.replace(
            self.session
                .files()
                .filter_map(|key| Some((key, self.session.source(key)?))),
        );
        files
    }

    /// The file `key` (its path from the root without `.qml`) now has
    /// this text; None: it is gone.
    pub fn save(&mut self, key: &str, text: Option<String>) {
        self.staged.insert(key.to_owned(), text);
    }

    /// Something happened to `path`. True when it is something a build
    /// has to look at; false for a path that is nothing to the project
    /// (an editor's swap file, a picture), and then nobody needs to wait
    /// for the quiet time.
    pub fn touch(&mut self, path: &Path, is_dir: bool) -> bool {
        let Some(root) = self.session.root() else {
            return false;
        };
        match classify(root, path, is_dir) {
            Touch::Ignore => return false,
            Touch::File => {
                self.touched.insert(path.to_owned());
            }
            Touch::Rescan => self.rescan = true,
            Touch::Modules => {
                self.rescan = true;
                self.force = true;
            }
        }
        true
    }

    /// The directory may have changed in ways no event told of.
    pub fn touch_all(&mut self) {
        self.rescan = true;
    }

    /// Whether a build would have anything to look at.
    pub fn has_saves(&self) -> bool {
        self.rescan || !self.staged.is_empty() || !self.touched.is_empty()
    }

    /// Reads what was touched and gives the session every save that says
    /// something new. True when one did.
    fn take_saves(&mut self) -> bool {
        let mut changed = std::mem::take(&mut self.force);
        if std::mem::take(&mut self.rescan) {
            // The directory is read whole, so what was touched is in it.
            self.touched.clear();
            if self.session.rescan().is_ok() {
                changed |= self.prints.replace(
                    self.session
                        .files()
                        .filter_map(|key| Some((key, self.session.source(key)?))),
                );
            }
        }
        for path in std::mem::take(&mut self.touched) {
            let Some(key) = self.session.key_of(&path) else {
                continue;
            };
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    self.staged.entry(key).or_insert(Some(text));
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    self.staged.entry(key).or_insert(None);
                }
                // Not readable now (being written, not text): the event
                // that follows the end of the write brings it back.
                Err(_) => {}
            }
        }
        for (key, text) in std::mem::take(&mut self.staged) {
            if !self.prints.differs(&key, text.as_deref()) {
                continue;
            }
            self.prints.record(&key, text.as_deref());
            match text {
                Some(text) => self.session.set_file(&key, text),
                None => self.session.remove_file(&key),
            }
            changed = true;
        }
        changed
    }

    /// Compiles what the saves since the last build changed.
    pub fn build(&mut self) -> Build {
        if !self.take_saves() {
            return Build::NoChange;
        }
        match self.session.build() {
            Ok(None) => Build::UpToDate,
            Ok(Some(patch)) => Build::Patch {
                patch: Box::new(patch),
                warnings: self.session.warnings().cloned().collect(),
            },
            Err(faults) => Build::Failed(faults),
        }
    }

    /// The application applied the patch of this generation.
    pub fn ack(&mut self, generation: u64) {
        self.session.ack(generation);
    }

    /// The application refused the patch of this generation and runs what
    /// it ran.
    pub fn reject(&mut self, generation: u64) {
        self.session.reject(generation);
    }
}

/// What a path under a project's root is to a build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Touch {
    /// Nothing: an editor's swap file, a picture, build output.
    Ignore,
    /// A QML file, to read again.
    File,
    /// A directory came or went: the files are found again.
    Rescan,
    /// A `qmldir` or a `CMakeLists.txt`: what a module has may differ
    /// although no QML file does.
    Modules,
}

/// Says what `path` is to the project under `root`. `is_dir` is what the
/// event knew; a path that is gone cannot be asked.
#[must_use]
pub fn classify(root: &Path, path: &Path, is_dir: bool) -> Touch {
    let Ok(relative) = path.strip_prefix(root) else {
        return Touch::Ignore;
    };
    let mut parts: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let Some(name) = parts.pop() else {
        // The root itself.
        return Touch::Rescan;
    };
    if parts.iter().any(|dir| skipped_dir(dir)) {
        return Touch::Ignore;
    }
    if is_dir || path.is_dir() {
        return if skipped_dir(&name) {
            Touch::Ignore
        } else {
            Touch::Rescan
        };
    }
    if name == "qmldir" || name == "CMakeLists.txt" {
        return Touch::Modules;
    }
    if name.starts_with('.') {
        return Touch::Ignore;
    }
    if name.ends_with(".qml") {
        return Touch::File;
    }
    if !name.contains('.') && !skipped_dir(&name) && !path.exists() {
        // A directory that was moved away or removed looks like this; the
        // event does not always say which it was.
        return Touch::Rescan;
    }
    Touch::Ignore
}

/// Directories the compiler does not look into, so nothing in them is a
/// change: build output, version control, an editor's own.
#[must_use]
pub fn skipped_dir(name: &str) -> bool {
    name.starts_with('.')
        || matches!(name, "target" | "node_modules" | "build")
        || name.starts_with("build-")
        || name.starts_with("cmake-build")
}

/// Every directory under `root` a project's files can be in, `root`
/// first: what a watcher has to watch.
pub fn project_dirs(root: &Path) -> Vec<PathBuf> {
    // More than this is not a QML project but a disk.
    const MAX_DIRS: usize = 4096;
    let mut out = Vec::new();
    let mut stack = vec![root.to_owned()];
    while let Some(dir) = stack.pop() {
        if out.len() >= MAX_DIRS {
            break;
        }
        if let Ok(read) = std::fs::read_dir(&dir) {
            for entry in read.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                // `file_type` does not follow links, so a link cannot
                // lead the walk out of the project.
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) && !skipped_dir(&name) {
                    stack.push(entry.path());
                }
            }
        }
        out.push(dir);
    }
    out.sort();
    out
}
