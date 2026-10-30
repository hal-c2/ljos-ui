//! From saved files to a patch: what the watcher's thread does once the
//! quiet time has passed, without a thread or a clock of its own, so a
//! test (and the scenario runner, where time is virtual) drives the same
//! code.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use ljos_compiler::{AliasError, Aliases, CompileFault, FileSet, ReloadSession};
use ljos_design::theme::Theme;
use ljos_ir::{CompiledFile, ReloadPatch};
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
    /// A file that had text was found empty while a save of it is under
    /// way: an editor that saves in place empties the file first, and on a
    /// busy disk the writing can come seconds later. Nothing was compiled.
    /// The next event of the file brings the build again; the owner sets
    /// no timer for it.
    Waiting,
    /// A change does not compile, and is not told yet: an editor can write
    /// a file in parts with a pause longer than the quiet time between
    /// them. The owner waits the quiet time again and builds: the files
    /// are read again, and that build tells the error if there still is
    /// one, as [`Build::Failed`].
    Unsettled,
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
    /// The files a save is under way of: touched since their writer last
    /// closed them, or emptied by a save that has not finished.
    unfinished: BTreeSet<String>,
    /// Paths found empty while being written, read again at their next
    /// event.
    parked: BTreeSet<PathBuf>,
    /// A build that waited gave the session saves: the next one compiles
    /// although it may bring nothing new.
    carried: bool,
    /// Pictures saved since they were last taken, to be shown again.
    pictures: BTreeSet<PathBuf>,
    /// The paths the last build read: read again by the build after a
    /// [`Build::Unsettled`].
    read: BTreeSet<PathBuf>,
    /// The faults of a [`Build::Unsettled`], told by the next build when
    /// the files still say the same.
    held: Option<Vec<CompileFault>>,
    /// The theme file, when the application has one where it looks.
    theme: Option<ThemeFile>,
}

/// A theme file ([`ljos_design::theme`]): not QML, so not the session's,
/// but saved and reloaded beside it.
#[derive(Debug)]
struct ThemeFile {
    /// What errors call it: its path from the root, or as it was given.
    name: String,
    /// Where it is read; None for one given by [`Reloader::save_theme`].
    path: Option<PathBuf>,
    /// A save given as text; the inner None is a file that is gone.
    staged: Option<Option<String>>,
    /// Something happened to the file: it is read at the next build.
    touched: bool,
    /// What the file said when last read, so a save that says the same
    /// reloads nothing.
    seen: Option<String>,
}

/// What became of the theme file since the last build.
pub type ThemeChange = Result<Option<Theme>, Vec<CompileFault>>;

impl<O: TypeOracle + Clone> Reloader<O> {
    /// Over files given by [`save`](Self::save).
    pub fn in_memory(oracle: O) -> Self {
        Self::over(ReloadSession::in_memory(oracle))
    }

    /// Over every QML file under `root`.
    pub fn open(root: &Path, oracle: O) -> Result<Self, ljos_compiler::Error> {
        Ok(Self::over(ReloadSession::open(root, oracle)?))
    }

    /// Over every QML file under each of `roots`, the import path of an
    /// application whose QML is spread over several trees:
    /// [`ReloadSession::open_roots`].
    pub fn open_roots(roots: &[&Path], oracle: O) -> Result<Self, ljos_compiler::Error> {
        Ok(Self::over(ReloadSession::open_roots(roots, oracle)?))
    }

    /// Over the files under `root` that the file `entry` (a key) reaches:
    /// [`ReloadSession::open_reaching`].
    pub fn open_reaching(
        root: &Path,
        entry: &str,
        oracle: O,
    ) -> Result<Self, ljos_compiler::Error> {
        Ok(Self::over(ReloadSession::open_reaching(
            root, entry, oracle,
        )?))
    }

    /// As [`open_roots`](Self::open_roots), with each module of `aliases`
    /// imported from its directory: [`ReloadSession::open_roots_aliased`].
    pub fn open_roots_aliased(
        roots: &[&Path],
        aliases: Aliases,
        oracle: O,
    ) -> Result<Self, ljos_compiler::Error> {
        Ok(Self::over(ReloadSession::open_roots_aliased(
            roots, aliases, oracle,
        )?))
    }

    /// As [`open_reaching`](Self::open_reaching), with each module of
    /// `aliases` imported from its directory:
    /// [`ReloadSession::open_reaching_aliased`].
    pub fn open_reaching_aliased(
        root: &Path,
        entry: &str,
        aliases: Aliases,
        oracle: O,
    ) -> Result<Self, ljos_compiler::Error> {
        Ok(Self::over(ReloadSession::open_reaching_aliased(
            root, entry, aliases, oracle,
        )?))
    }

    fn over(session: ReloadSession<O>) -> Self {
        Self {
            session,
            prints: Fingerprints::new(),
            staged: BTreeMap::new(),
            touched: BTreeSet::new(),
            rescan: false,
            force: false,
            unfinished: BTreeSet::new(),
            parked: BTreeSet::new(),
            carried: false,
            pictures: BTreeSet::new(),
            read: BTreeSet::new(),
            held: None,
            theme: None,
        }
    }

    /// Watches the theme file the application started with: `name` is
    /// what errors call it, `path` where it is read (None when it is
    /// given by [`save_theme`](Self::save_theme)), `text` what it said at
    /// the start, None when there was none.
    pub fn watch_theme(&mut self, name: &str, path: Option<PathBuf>, text: Option<String>) {
        self.theme = Some(ThemeFile {
            name: name.to_owned(),
            path,
            staged: None,
            touched: false,
            seen: text,
        });
    }

    /// Where the theme file is read, to be watched.
    pub fn theme_path(&self) -> Option<&Path> {
        self.theme.as_ref()?.path.as_deref()
    }

    /// The theme file now says `text`; None: it is gone.
    pub fn save_theme(&mut self, text: Option<String>) {
        if let Some(t) = &mut self.theme {
            t.staged = Some(text);
        }
    }

    /// What the theme file says now, when it says something else than it
    /// did: `Ok(None)` when it is gone, `Err` when it cannot be read as a
    /// theme. None when nothing changed. The QML of the same saves is
    /// built by [`build`](Self::build).
    pub fn take_theme(&mut self) -> Option<ThemeChange> {
        let t = self.theme.as_mut()?;
        let text = match (t.staged.take(), std::mem::take(&mut t.touched), &t.path) {
            (Some(text), ..) => text,
            (None, true, Some(path)) => match std::fs::read_to_string(path) {
                Ok(text) => Some(text),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                // Not readable now (being written): the event that ends the
                // write brings it back.
                Err(_) => return None,
            },
            _ => return None,
        };
        if text == t.seen {
            return None;
        }
        t.seen.clone_from(&text);
        Some(match text {
            None => Ok(None),
            Some(text) => crate::host::parse_theme(&t.name, &text),
        })
    }

    /// Takes the generation a theme reload is applied at, to be answered
    /// by [`ack`](Self::ack) or [`reject`](Self::reject) as a patch is.
    pub fn advance(&mut self) -> u64 {
        self.session.advance()
    }

    /// [`ReloadSession::set_controls_style`].
    pub fn set_controls_style(&mut self, style: Option<qml_front::project::ControlsStyle>) {
        self.session.set_controls_style(style);
    }

    pub fn session(&self) -> &ReloadSession<O> {
        &self.session
    }

    /// The directory of a reloader made by [`open`](Self::open).
    pub fn root(&self) -> Option<&Path> {
        self.session.root()
    }

    /// Every directory the files are found in, earlier first.
    pub fn roots(&self) -> &[PathBuf] {
        self.session.roots()
    }

    /// Every directory a file of the project may be in, and so every one
    /// a watcher watches: [`ReloadSession::places`].
    pub fn places(&self) -> Vec<PathBuf> {
        self.session.places()
    }

    /// Imports each module of `aliases` from its directory from the next
    /// build on, which reads the directories again and compiles every
    /// file, as a changed `qmldir` does: [`ReloadSession::set_aliases`].
    ///
    /// # Errors
    /// A directory neither inside a root nor one the reloader started
    /// with; nothing changes then.
    pub fn set_aliases(&mut self, aliases: Aliases) -> Result<(), AliasError> {
        self.session.set_aliases(aliases)?;
        self.rescan = true;
        self.force = true;
        Ok(())
    }

    /// Whether the next build reads the directory again: there may be
    /// directories to watch that were not there before.
    pub fn rescan_pending(&self) -> bool {
        self.rescan
    }

    /// Compiles everything, with every save so far, as what the
    /// application starts from. A file that does not compile is not among
    /// them: [`ReloadSession::failures`] says why, and a build offers it
    /// once it compiles.
    pub fn start(&mut self) -> FileSet {
        self.take_saves(false);
        let files = self.session.start().clone();
        self.prints.replace(texts(&self.session));
        files
    }

    /// The file `key` (its path from the root without `.qml`, or with its
    /// suffix for a JavaScript file) now has this text; None: it is gone.
    pub fn save(&mut self, key: &str, text: Option<String>) {
        self.unfinished.remove(key);
        self.staged.insert(key.to_owned(), text);
    }

    /// The file `key` was emptied by a save that has not finished: its
    /// text comes with a later [`save`](Self::save).
    pub fn emptied(&mut self, key: &str) {
        self.unfinished.insert(key.to_owned());
        self.staged.insert(key.to_owned(), Some(String::new()));
    }

    /// Something happened to `path`. True when it is something a build
    /// has to look at; false for a path that is nothing to the project
    /// (an editor's swap file, build output), and then nobody needs to
    /// wait for the quiet time. A picture is no build's, but it waits for
    /// the quiet time too: [`take_pictures`](Self::take_pictures).
    pub fn touch(&mut self, path: &Path, is_dir: bool) -> bool {
        if let Some(t) = &mut self.theme
            && t.path.as_deref() == Some(path)
        {
            t.touched = true;
            return true;
        }
        let places = self.session.places();
        let Some(root) = root_of(&places, path) else {
            return false;
        };
        match classify(root, path, is_dir) {
            Touch::Ignore => return false,
            Touch::Picture => {
                self.pictures.insert(path.to_owned());
            }
            Touch::File => {
                if let Some(key) = self.session.key_of(path) {
                    self.unfinished.insert(key);
                }
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

    /// A writer closed `path` after writing it: what it holds now is the
    /// whole save, an empty file too. True as for [`touch`](Self::touch).
    pub fn closed(&mut self, path: &Path) -> bool {
        let touched = self.touch(path, false);
        if let Some(key) = self.session.key_of(path) {
            self.unfinished.remove(&key);
        }
        touched
    }

    /// The pictures saved since the last call: an application shows them
    /// again, and no build has anything to do with them.
    pub fn take_pictures(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.pictures).into_iter().collect()
    }

    /// The directory may have changed in ways no event told of.
    pub fn touch_all(&mut self) {
        self.rescan = true;
        if let Some(t) = &mut self.theme {
            t.touched = true;
        }
    }

    /// Whether a build would have anything to look at.
    pub fn has_saves(&self) -> bool {
        self.rescan
            || !self.staged.is_empty()
            || !self.touched.is_empty()
            || self
                .theme
                .as_ref()
                .is_some_and(|t| t.touched || t.staged.is_some())
    }

    /// Whether `text`, a save of `key`, is a file emptied by a save that
    /// has not finished.
    fn half_saved(&self, key: &str, text: &str) -> bool {
        text.is_empty()
            && self.unfinished.contains(key)
            && self.session.source(key).is_some_and(|s| !s.is_empty())
    }

    /// Reads what was touched and gives the session every save that says
    /// something new: whether one did, and whether a file was left for its
    /// save to finish ([`Build::Waiting`]). `may_wait` false takes every
    /// file as it is now.
    fn take_saves(&mut self, may_wait: bool) -> (bool, bool) {
        let mut changed = std::mem::take(&mut self.force) | std::mem::take(&mut self.carried);
        let mut waiting = false;
        if !may_wait {
            self.unfinished.clear();
            let parked = std::mem::take(&mut self.parked);
            self.touched.extend(parked);
        }
        if std::mem::take(&mut self.rescan) {
            // The directory is read whole, so what was touched is in it.
            self.touched.clear();
            if self.session.rescan().is_ok() {
                changed |= self.prints.replace(texts(&self.session));
            }
        }
        self.read.clear();
        for path in std::mem::take(&mut self.touched) {
            let Some(key) = self.session.key_of(&path) else {
                continue;
            };
            self.read.insert(path.clone());
            self.parked.remove(&path);
            // With more than one root, the file that has the key may be
            // another root's.
            let read = self.session.path_of(&key).unwrap_or_else(|| path.clone());
            match std::fs::read_to_string(&read) {
                Ok(text) if self.half_saved(&key, &text) => {
                    self.parked.insert(path);
                    waiting = true;
                }
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
        let mut kept = BTreeMap::new();
        for (key, text) in std::mem::take(&mut self.staged) {
            if text.as_deref().is_some_and(|t| self.half_saved(&key, t)) {
                kept.insert(key, text);
                waiting = true;
                continue;
            }
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
        // Nothing of a change is compiled while part of it is unwritten.
        self.staged.extend(kept);
        self.carried = waiting && changed;
        (changed, waiting)
    }

    /// Compiles what the saves since the last build changed.
    pub fn build(&mut self) -> Build {
        self.build_with(true)
    }

    /// As [`build`](Self::build), taking every file as it is now, an empty
    /// one whose save has not finished too: a reload was asked for.
    pub fn build_now(&mut self) -> Build {
        self.build_with(false)
    }

    fn build_with(&mut self, may_wait: bool) -> Build {
        let (changed, waiting) = self.take_saves(may_wait);
        if waiting {
            return Build::Waiting;
        }
        if !changed {
            // Read again, the files say what they said: the error stands.
            return self.held.take().map_or(Build::NoChange, Build::Failed);
        }
        let built = self.session.build();
        let held = self.held.take();
        match built {
            Ok(None) => Build::UpToDate,
            Ok(Some(patch)) => Build::Patch {
                patch: Box::new(patch),
                warnings: self.session.warnings().cloned().collect(),
            },
            // Once only, and not when a reload was asked for.
            Err(faults) if may_wait && held.is_none() => {
                let read = std::mem::take(&mut self.read);
                // What a rescan found is found by the next one.
                self.rescan |= read.is_empty() && !self.session.roots().is_empty();
                self.touched.extend(read);
                self.held = Some(faults);
                Build::Unsettled
            }
            Err(faults) => Build::Failed(faults),
        }
    }

    /// The file `key` for a program that makes an object of it although
    /// no file of the session imports it: [`ReloadSession::provide`].
    pub fn provide(&mut self, key: &str) -> Option<Result<Arc<CompiledFile>, String>> {
        let known = self.session.files().count();
        let file = self.session.provide(key);
        if self.session.files().count() != known {
            // What was read is what a save is told from.
            self.prints.replace(texts(&self.session));
        }
        file
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

/// Every file of the session by key with its text, the JavaScript files
/// too.
fn texts<O: TypeOracle + Clone>(session: &ReloadSession<O>) -> impl Iterator<Item = (&str, &str)> {
    session
        .files()
        .filter_map(|key| Some((key, session.source(key)?)))
        .chain(session.scripts())
}

/// What a path under a project's root is to a build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Touch {
    /// Nothing: an editor's swap file, build output.
    Ignore,
    /// A picture an image can show: it is shown again, not compiled.
    Picture,
    /// A QML or JavaScript file, to read again.
    File,
    /// A directory came or went: the files are found again.
    Rescan,
    /// A `qmldir` or a `CMakeLists.txt`: what a module has may differ
    /// although no QML file does.
    Modules,
}

/// The one of `roots` that `path` is under.
#[must_use]
pub fn root_of<'a>(roots: &'a [PathBuf], path: &Path) -> Option<&'a Path> {
    roots
        .iter()
        .find(|root| path.starts_with(root))
        .map(PathBuf::as_path)
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
    if name.ends_with(".qml") || name.ends_with(".js") || name.ends_with(".mjs") {
        return Touch::File;
    }
    if ljos_quick::ImageStore::is_picture_file(path) {
        return Touch::Picture;
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
pub use ljos_compiler::skipped_dir;

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
