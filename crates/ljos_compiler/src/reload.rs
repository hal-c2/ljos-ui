//! Dev reload on the compiler's side (ARCHITECTURE.md §12.1): what changed
//! between two compilations of a project, as a [`ReloadPatch`], and the
//! [`ReloadSession`] that keeps a project compiled while its files change.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use ljos_ir::{CompiledFile, ReloadPatch};
use qml_front::TypeOracle;
use qml_front::project::is_script;

use crate::{Diagnostic, Error, Project, Severity, compile_file, reload_lint};

/// The compiled files of a project, by key.
pub type FileSet = BTreeMap<String, Arc<CompiledFile>>;

mod diff;

pub use diff::{diff, diff_file};

/// Something wrong with a file, where a developer finds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileFault {
    /// The file's path from the project's root, with `.qml` (or the
    /// script's own suffix).
    pub file: String,
    /// 1-based; 0 when the place is not known.
    pub line: u32,
    pub column: u32,
    pub message: String,
}

impl std::fmt::Display for CompileFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}",
            self.file, self.line, self.column, self.message
        )
    }
}

impl CompileFault {
    /// Why the file cannot be made, as a loader says it after the file's
    /// URL ([`crate::failure_reason`]).
    #[must_use]
    pub fn reason(&self) -> String {
        if self.line == 0 {
            self.message.clone()
        } else {
            format!("{}:{}: {}", self.line, self.column, self.message)
        }
    }

    fn of(key: &str, d: &Diagnostic) -> Self {
        Self {
            file: format!("{key}.qml"),
            line: d.span.line,
            column: d.span.column,
            message: d.message.clone(),
        }
    }
}

/// A project kept compiled while its files change.
///
/// It knows what the application runs (the set it was last told was
/// applied) and what the files now say, and gives the patch from the one to
/// the other. Only what a change can affect is compiled again: the changed
/// files and the files that name them.
///
/// A file that does not compile when the session starts does not stop it:
/// the application runs without it, as a program that never makes an object
/// of the file runs, and [`failures`](Self::failures) says why it is
/// missing. Such a file stops no later build until it is saved; a save that
/// does not compile is refused, as any other is.
#[derive(Debug)]
pub struct ReloadSession<O> {
    oracle: O,
    /// The import path the files are found on, earlier first: one
    /// directory, or the several an application's QML is spread over.
    roots: Vec<PathBuf>,
    sources: BTreeMap<String, String>,
    project: Project<O>,
    /// What the application runs.
    live: FileSet,
    /// The files the application runs without, because they did not
    /// compile.
    live_failed: BTreeSet<String>,
    /// The files that do not compile now and are let be: they did not
    /// compile in what the application runs either, and were not saved
    /// since.
    failed: BTreeMap<String, Vec<CompileFault>>,
    /// What the sources say, for every file that is not dirty.
    compiled: FileSet,
    dirty: BTreeSet<String>,
    all_dirty: bool,
    /// The names only a running program resolves, at the last build: when
    /// the set changes, every file may compile differently.
    dynamic: Option<HashSet<String>>,
    generation: u64,
    last: u64,
    pending: Option<(u64, FileSet, BTreeSet<String>)>,
    warnings: BTreeMap<String, Vec<CompileFault>>,
    /// The scripts saved since the start that do not parse: no build is
    /// offered while one is.
    broken_scripts: BTreeMap<String, CompileFault>,
    /// For a session [`open_reaching`](Self::open_reaching): the files
    /// the program was found to reach, which a rescan reaches again. None:
    /// every file of the directory is the session's.
    reached: Option<BTreeSet<String>>,
}

impl<O: TypeOracle + Clone> ReloadSession<O> {
    /// A session over files given by [`set_file`](Self::set_file).
    pub fn in_memory(oracle: O) -> Self {
        Self {
            project: Project::new(oracle.clone()),
            oracle,
            roots: Vec::new(),
            sources: BTreeMap::new(),
            live: FileSet::new(),
            live_failed: BTreeSet::new(),
            failed: BTreeMap::new(),
            compiled: FileSet::new(),
            dirty: BTreeSet::new(),
            all_dirty: true,
            dynamic: None,
            generation: 0,
            last: 0,
            pending: None,
            warnings: BTreeMap::new(),
            broken_scripts: BTreeMap::new(),
            reached: None,
        }
    }

    /// Chooses the style behind `import QtQuick.Controls`, as a program
    /// chooses one when it starts: for a session whose files are given,
    /// which reads no configuration. Every file compiles again.
    pub fn set_controls_style(&mut self, style: Option<qml_front::project::ControlsStyle>) {
        self.project.set_controls_style(style);
        self.all_dirty = true;
    }

    /// A session over every QML file under `root`, as [`Project::load`]
    /// finds them.
    pub fn open(root: &Path, oracle: O) -> Result<Self, Error> {
        let mut session = Self::in_memory(oracle);
        session.roots = vec![root.to_owned()];
        session.rescan()?;
        Ok(session)
    }

    /// A session over every QML file under each of `roots`, as
    /// [`Project::load_roots`] finds them: a file of one root may import
    /// a module of another, and where two have a file of one key the
    /// earlier root's is the one compiled.
    pub fn open_roots(roots: &[&Path], oracle: O) -> Result<Self, Error> {
        let mut session = Self::in_memory(oracle);
        session.roots = roots.iter().map(|r| (*r).to_owned()).collect();
        session.rescan()?;
        Ok(session)
    }

    /// A session over the files under `root` that the file `entry` (a
    /// key) reaches, as a program loads them ([`Project::reach`]): a file
    /// nothing reaches is not read, compiled or told of. A save that
    /// imports more reads more, and so does a file the program makes an
    /// object of when it runs ([`provide`](Self::provide)).
    pub fn open_reaching(root: &Path, entry: &str, oracle: O) -> Result<Self, Error> {
        let mut session = Self::in_memory(oracle);
        session.roots = vec![root.to_owned()];
        session.reached = Some(BTreeSet::from([entry.to_owned()]));
        session.rescan()?;
        Ok(session)
    }

    /// The directory of a session made by [`open`](Self::open); the
    /// first of [`open_roots`](Self::open_roots)'.
    pub fn root(&self) -> Option<&Path> {
        self.roots.first().map(PathBuf::as_path)
    }

    /// Every directory the session's files are found in, earlier first.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Reads the directory again: for a file that was added or removed, and
    /// for a `qmldir` or a `CMakeLists.txt` that changed what a module has.
    pub fn rescan(&mut self) -> Result<(), Error> {
        let Some(root) = self.roots.first().cloned() else {
            return Ok(());
        };
        self.project = match &self.reached {
            Some(reached) => {
                let mut project = Project::open(&root, self.oracle.clone())?;
                for key in reached {
                    project.reach(key);
                }
                project
            }
            None => {
                let roots: Vec<&Path> = self.roots.iter().map(PathBuf::as_path).collect();
                Project::load_roots(&roots, self.oracle.clone())?
            }
        };
        // A broken script stays refused until it is mended or gone.
        let project = &self.project;
        self.broken_scripts.retain(|key, _| {
            project.script(key).is_some_and(|text| {
                crate::script::syntax_error(text, key.ends_with(".mjs")).is_some()
            })
        });
        self.sources = self
            .project
            .files()
            .map(|key| {
                let text = self.project.source(key).unwrap_or_default().to_owned();
                (key.to_owned(), text)
            })
            .collect();
        self.all_dirty = true;
        Ok(())
    }

    /// The key of a path under the session's root: its path from there
    /// without `.qml`, or with its suffix for a JavaScript file. None for
    /// a path that is neither of the project.
    pub fn key_of(&self, path: &Path) -> Option<String> {
        let relative = self
            .roots
            .iter()
            .find_map(|root| path.strip_prefix(root).ok())?;
        let text = relative.to_string_lossy().replace('\\', "/");
        if is_script(&text) {
            return Some(text);
        }
        text.strip_suffix(".qml").map(str::to_owned)
    }

    /// The file the text of `key` is read from: under the first root
    /// that has one, as the compiler finds it, so a save to a file
    /// another root's hides is no change, and the file that was hidden
    /// is what a key has once the one before it is gone. None for a
    /// session over files in memory.
    pub fn path_of(&self, key: &str) -> Option<PathBuf> {
        let name = if is_script(key) {
            key.to_owned()
        } else {
            format!("{key}.qml")
        };
        let mut paths = self.roots.iter().map(|root| root.join(&name));
        let first = paths.next()?;
        if self.roots.len() == 1 || first.exists() {
            return Some(first);
        }
        Some(paths.find(|p| p.exists()).unwrap_or(first))
    }

    /// The file `key` now has this text. Whether it is new or not. A key
    /// ending in `.js` or `.mjs` is a JavaScript file.
    pub fn set_file(&mut self, key: &str, source: impl Into<String>) {
        let source = source.into();
        if is_script(key) {
            if self.project.script(key) != Some(source.as_str()) {
                match crate::script::syntax_error(&source, key.ends_with(".mjs")) {
                    Some((line, column, message)) => {
                        let file = key.to_owned();
                        let fault = CompileFault {
                            file,
                            line,
                            column,
                            message,
                        };
                        self.broken_scripts.insert(key.to_owned(), fault);
                    }
                    None => {
                        self.broken_scripts.remove(key);
                    }
                }
                self.project.add_script(key, source);
                self.script_changed(key);
            }
            return;
        }
        if self.sources.get(key) == Some(&source) {
            return;
        }
        if !self.project.has_read(qml_front::project::directory(key)) {
            // Nothing reaches it: it is read from the disk when something
            // does.
            return;
        }
        if !self.sources.contains_key(key) {
            // Any file may have been waiting for a type of this name.
            self.all_dirty = true;
        }
        self.project.add(key, source.as_str());
        self.sources.insert(key.to_owned(), source);
        self.dirty.insert(key.to_owned());
        // It may import what nothing did before, and a file may have been
        // waiting for a type of a name read with it.
        for new in self.settle(key) {
            self.dirty.insert(new);
            self.all_dirty = true;
        }
    }

    /// Reads what `key` reaches that the session did not have: the keys
    /// read.
    fn settle(&mut self, key: &str) -> Vec<String> {
        let read = self.project.reach(key);
        for new in &read {
            let text = self.project.source(new).unwrap_or_default().to_owned();
            self.sources.insert(new.clone(), text);
        }
        read
    }

    /// The file `key` for a program that makes an object of it although
    /// nothing the session has imports it: a `Loader` or
    /// `Qt.createComponent` given its URL. It is read with what it
    /// reaches and compiled, and the application runs it from then on, so
    /// a save of it reloads. None when there is no such file, `Err` with
    /// why when it does not compile.
    pub fn provide(&mut self, key: &str) -> Option<Result<Arc<CompiledFile>, String>> {
        if let Some(file) = self.live.get(key) {
            return Some(Ok(file.clone()));
        }
        if self.live_failed.contains(key) {
            let first = self.failed.get(key).and_then(|f| f.first());
            return Some(Err(first.map_or_else(String::new, CompileFault::reason)));
        }
        // What runs was compiled without these files, as Qt's would be:
        // nothing is compiled again for them until a save.
        self.settle(key);
        if !self.sources.contains_key(key) {
            return None;
        }
        if let Some(reached) = &mut self.reached {
            reached.insert(key.to_owned());
        }
        let compiled = self.compile_one(key);
        let pending = self
            .pending
            .as_mut()
            .map(|(_, files, failed)| (files, failed));
        match compiled {
            Ok((file, warnings)) => {
                self.live.insert(key.to_owned(), file.clone());
                self.compiled.insert(key.to_owned(), file.clone());
                self.warnings.insert(key.to_owned(), warnings);
                if let Some((files, _)) = pending {
                    files.insert(key.to_owned(), file.clone());
                }
                Some(Ok(file))
            }
            Err(errors) => {
                let reason = errors
                    .first()
                    .map_or_else(String::new, CompileFault::reason);
                self.live_failed.insert(key.to_owned());
                self.failed.insert(key.to_owned(), errors);
                if let Some((_, failed)) = pending {
                    failed.insert(key.to_owned());
                }
                Some(Err(reason))
            }
        }
    }

    /// The file `key` is gone.
    pub fn remove_file(&mut self, key: &str) {
        if is_script(key) {
            self.broken_scripts.remove(key);
            if self.project.script(key).is_some() {
                self.project.remove_script(key);
                self.script_changed(key);
            }
            return;
        }
        if self.sources.remove(key).is_none() {
            return;
        }
        if self.reached.is_some() || !self.roots.is_empty() {
            // What its tree or its directories say of the modules is kept.
            self.project.remove(key);
            self.all_dirty = true;
            return;
        }
        // A project cannot forget a file, so it is made again without it.
        // A module's types come from the directory, which a session over
        // files in memory does not have.
        let mut project = Project::new(self.oracle.clone());
        for (key, source) in &self.sources {
            project.add(key, source.as_str());
        }
        for (path, text) in self.project.scripts() {
            project.add_script(path, text);
        }
        self.project = project;
        self.all_dirty = true;
    }

    /// The text of a file as the session has it, a JavaScript file's too.
    pub fn source(&self, key: &str) -> Option<&str> {
        if is_script(key) {
            return self.project.script(key);
        }
        self.sources.get(key).map(String::as_str)
    }

    /// The JavaScript files: their keys and their text.
    pub fn scripts(&self) -> impl Iterator<Item = (&str, &str)> {
        self.project.scripts()
    }

    /// The script `key` changed: a QML file that names it is compiled
    /// again, as the script is part of what it compiles to. A script may
    /// import another, so when one names it every file is.
    fn script_changed(&mut self, key: &str) {
        let name = key.rsplit('/').next().unwrap_or(key);
        if self
            .project
            .scripts()
            .any(|(other, text)| other != key && text.contains(name))
        {
            self.all_dirty = true;
            return;
        }
        for (file, source) in &self.sources {
            if source.contains(name) {
                self.dirty.insert(file.clone());
            }
        }
    }

    /// The keys of the files, in order.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.sources.keys().map(String::as_str)
    }

    /// What the application runs: the files of the last acknowledged build.
    pub fn live(&self) -> &FileSet {
        &self.live
    }

    /// The files that do not compile, with why, which the application runs
    /// without: whatever makes an object of one is told the first reason.
    pub fn failures(&self) -> impl Iterator<Item = (&str, &[CompileFault])> {
        self.failed.iter().map(|(k, f)| (k.as_str(), f.as_slice()))
    }

    /// The generation the application is at.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Warnings of the files as they now are: what the compiler could not
    /// settle, and the reload lint.
    pub fn warnings(&self) -> impl Iterator<Item = &CompileFault> {
        self.warnings.values().flatten()
    }

    /// Compiles what changed. `Ok(None)` when the files say what the
    /// application already runs; `Err` with every error when a file does
    /// not compile, and then nothing of the change is offered.
    pub fn build(&mut self) -> Result<Option<ReloadPatch>, Vec<CompileFault>> {
        // What imports a broken script would run without its functions.
        if !self.broken_scripts.is_empty() {
            return Err(self.broken_scripts.values().cloned().collect());
        }
        self.compile_dirty(false)?;
        let failed: BTreeSet<String> = self.failed.keys().cloned().collect();
        if failed == self.live_failed
            && self.compiled.len() == self.live.len()
            && self
                .compiled
                .iter()
                .zip(&self.live)
                .all(|((ka, a), (kb, b))| ka == kb && (Arc::ptr_eq(a, b) || a == b))
        {
            self.pending = None;
            return Ok(None);
        }
        self.last += 1;
        let mut patch = diff(&self.live, &self.compiled, self.generation, self.last);
        // A file that never compiled and is gone now: the application
        // forgets why it was missing.
        for key in self.live_failed.difference(&failed) {
            if !self.compiled.contains_key(key) && !patch.removed.contains(key) {
                patch.removed.push(key.clone());
            }
        }
        self.pending = Some((self.last, self.compiled.clone(), failed));
        Ok(Some(patch))
    }

    /// Compiles everything and takes it as what the application runs, at
    /// generation 0: how a session starts. A file that does not compile is
    /// left out and told by [`failures`](Self::failures).
    pub fn start(&mut self) -> &FileSet {
        self.all_dirty = true;
        if let Err(faults) = self.compile_dirty(true) {
            unreachable!("a start lets every file fail: {faults:?}");
        }
        self.live = self.compiled.clone();
        self.live_failed = self.failed.keys().cloned().collect();
        self.pending = None;
        &self.live
    }

    /// The application applied the patch of this generation.
    pub fn ack(&mut self, generation: u64) {
        if let Some((g, files, failed)) = self.pending.take() {
            if g == generation {
                self.live = files;
                self.live_failed = failed;
                self.generation = generation;
            } else {
                self.pending = Some((g, files, failed));
            }
        }
    }

    /// The application refused the patch of this generation: the next build
    /// starts again from what it runs.
    pub fn reject(&mut self, generation: u64) {
        if self
            .pending
            .as_ref()
            .is_some_and(|(g, ..)| *g == generation)
        {
            self.pending = None;
        }
    }

    /// `tolerate`: every file may fail, as at the start. Otherwise only a
    /// file the application already runs without, and which was not saved
    /// since, may.
    fn compile_dirty(&mut self, tolerate: bool) -> Result<(), Vec<CompileFault>> {
        let dynamic = self.project.front().dynamic_names(self.project.oracle());
        if self.dynamic.as_ref() != Some(&dynamic) {
            self.all_dirty = true;
        }
        let todo: BTreeSet<String> = if self.all_dirty {
            self.sources.keys().cloned().collect()
        } else {
            self.affected()
        };
        let mut faults = Vec::new();
        let mut done: Vec<(String, Arc<CompiledFile>, Vec<CompileFault>)> = Vec::new();
        let mut let_be: Vec<(String, Vec<CompileFault>)> = Vec::new();
        for key in &todo {
            let errors = match self.compile_one(key) {
                Ok((file, warnings)) => {
                    done.push((key.clone(), file, warnings));
                    continue;
                }
                Err(errors) => errors,
            };
            if tolerate || (self.live_failed.contains(key) && !self.dirty.contains(key)) {
                let_be.push((key.clone(), errors));
            } else {
                faults.extend(errors);
            }
        }
        if !faults.is_empty() {
            // What is dirty stays dirty: the next build tries all of it again.
            return Err(faults);
        }
        for (key, file, warnings) in done {
            self.failed.remove(&key);
            self.compiled.insert(key.clone(), file);
            self.warnings.insert(key, warnings);
        }
        for (key, errors) in let_be {
            self.compiled.remove(&key);
            self.warnings.remove(&key);
            self.failed.insert(key, errors);
        }
        self.compiled
            .retain(|key, _| self.sources.contains_key(key));
        self.failed.retain(|key, _| self.sources.contains_key(key));
        self.warnings
            .retain(|key, _| self.sources.contains_key(key));
        for key in &todo {
            let Some(file) = self.compiled.get(key) else {
                continue;
            };
            let lint = reload_lint::lint(file, &|other| self.compiled.get(other).map(Arc::as_ref));
            let lint = lint.iter().map(|d| CompileFault::of(key, d));
            self.warnings.entry(key.clone()).or_default().extend(lint);
        }
        self.dirty.clear();
        self.all_dirty = false;
        self.dynamic = Some(dynamic);
        Ok(())
    }

    /// The file `key` compiled, with its warnings; or its errors.
    fn compile_one(
        &self,
        key: &str,
    ) -> Result<(Arc<CompiledFile>, Vec<CompileFault>), Vec<CompileFault>> {
        match compile_file(&self.project, key) {
            Err(diagnostics) => {
                let mut errors: Vec<CompileFault> = diagnostics
                    .iter()
                    .map(|d| CompileFault::of(key, d))
                    .collect();
                if errors.is_empty() {
                    errors.push(CompileFault {
                        file: format!("{key}.qml"),
                        line: 0,
                        column: 0,
                        message: "the file does not compile".to_owned(),
                    });
                }
                Err(errors)
            }
            Ok(c) => {
                let (errors, warnings): (Vec<_>, Vec<_>) = c
                    .diagnostics
                    .iter()
                    .partition(|d| d.severity == Severity::Error);
                if errors.is_empty() {
                    let warnings = warnings
                        .into_iter()
                        .map(|d| CompileFault::of(key, d))
                        .collect();
                    Ok((Arc::new(c.file), warnings))
                } else {
                    Err(errors
                        .into_iter()
                        .map(|d| CompileFault::of(key, d))
                        .collect())
                }
            }
        }
    }

    /// The dirty files and every file that may compile differently because
    /// of them: a file names another by its type, so one that does not have
    /// the type's name in its text cannot depend on it. A file that is a
    /// type of a module may go by another name there, so then every file
    /// is taken.
    fn affected(&self) -> BTreeSet<String> {
        let mut out: BTreeSet<String> = BTreeSet::new();
        let mut todo: Vec<String> = self.dirty.iter().cloned().collect();
        while let Some(key) = todo.pop() {
            if !self.sources.contains_key(&key) || !out.insert(key.clone()) {
                continue;
            }
            if !self.project.front().modules_of(&key).is_empty() {
                return self.sources.keys().cloned().collect();
            }
            let name = key.rsplit('/').next().unwrap_or(&key);
            for (other, source) in &self.sources {
                if !out.contains(other) && source.contains(name) {
                    todo.push(other.clone());
                }
            }
        }
        out
    }
}
