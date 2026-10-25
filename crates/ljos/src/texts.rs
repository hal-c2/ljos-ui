//! `Qt.createQmlObject` in an application built with `dev-qml`, against
//! the files on disk as they are when the text is made, not as they were
//! when the application started: a file added to one of its directories
//! since then is a type a text may name, and a text placed in a directory
//! of its own (a user's file read at run time) finds the files beside it.
//!
//! Nothing watches the disk. The directories are read at the first text;
//! at each later one the time of every directory read is looked at, one
//! `stat` each, and the files are read again only when one changed (a
//! file was added, removed or renamed in it). The component source the
//! engine loads from is the one the application started with; a key it
//! lacks is compiled from what the texts read, once, and only after a
//! text was made, so a program that makes none reads nothing.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::SystemTime;

use ljos_compiler::{Project, compile_file, compile_text};
use ljos_engine::{ComponentSource, Engine, MapSource, QmlText, QmlTextCompiler, QmlTextError};
use ljos_ir::CompiledFile;
use ljos_js::Bytecode;

use crate::compile::EngineOracle;

/// The compiler and the component source of one application, over the
/// same files.
pub(crate) struct DiskTexts {
    state: Rc<RefCell<State>>,
}

struct State {
    dirs: Vec<PathBuf>,
    oracle: EngineOracle,
    project: Option<Project<EngineOracle>>,
    /// Every directory read, with its time when it was read.
    stamps: Vec<(PathBuf, Option<SystemTime>)>,
    /// Directories outside `dirs` that a text was placed in, with the key
    /// their files have: the text's own key without its name.
    outside: Vec<(PathBuf, String)>,
    /// What the engine loaded that its own source lacks.
    made: HashMap<String, Arc<CompiledFile>>,
    failed: HashMap<String, String>,
    /// The URLs of the files outside `dirs`, which no root keys.
    urls: HashMap<String, String>,
}

impl DiskTexts {
    pub(crate) fn new(dirs: &[&Path], oracle: EngineOracle) -> Self {
        Self {
            state: Rc::new(RefCell::new(State {
                dirs: dirs.iter().map(|d| d.to_path_buf()).collect(),
                oracle,
                project: None,
                stamps: Vec::new(),
                outside: Vec::new(),
                made: HashMap::new(),
                failed: HashMap::new(),
                urls: HashMap::new(),
            })),
        }
    }

    /// Gives `engine` the components of `source`, and whatever the texts
    /// read beyond them, and the compiler for the texts.
    pub(crate) fn install(self, engine: &Engine, source: Option<MapSource>) {
        self.install_with(engine, Rc::new(source.unwrap_or_default()));
    }

    /// [`DiskTexts::install`] over the source the engine has now, for an
    /// engine set up by hand.
    pub(crate) fn install_over(self, engine: &Engine) {
        let source = engine
            .component_source()
            .unwrap_or_else(|| Rc::new(MapSource::default()));
        self.install_with(engine, source);
    }

    fn install_with(self, engine: &Engine, source: Rc<dyn ComponentSource>) {
        engine.set_component_source(Fallback {
            source,
            state: Rc::clone(&self.state),
        });
        let state = self.state;
        let compiler: QmlTextCompiler =
            Rc::new(move |text: QmlText<'_>| state.borrow_mut().compile(text));
        engine.set_qml_text_compiler(Some(compiler));
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// `root` and every directory below it the compiler reads.
fn directories(root: &Path, out: &mut Vec<PathBuf>) {
    out.push(root.to_owned());
    let Ok(read) = std::fs::read_dir(root) else {
        return;
    };
    for entry in read.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let skipped = name.starts_with('.')
            || matches!(&*name, "target" | "node_modules" | "build")
            || name.starts_with("build-")
            || name.starts_with("cmake-build");
        if !skipped && entry.file_type().is_ok_and(|t| t.is_dir()) {
            directories(&entry.path(), out);
        }
    }
}

fn plain(message: String) -> Vec<QmlTextError> {
    vec![QmlTextError {
        line: 0,
        column: 0,
        message,
    }]
}

impl State {
    fn stale(&self) -> bool {
        self.project.is_none() || self.stamps.iter().any(|(dir, at)| modified(dir) != *at)
    }

    /// The project, read again when a directory changed since it was.
    fn fresh(&mut self) -> Result<&mut Project<EngineOracle>, String> {
        if self.stale() {
            let roots: Vec<&Path> = self.dirs.iter().map(PathBuf::as_path).collect();
            let mut project =
                Project::load_roots(&roots, self.oracle.clone()).map_err(|e| e.to_string())?;
            let mut dirs = Vec::new();
            for root in &self.dirs {
                directories(root, &mut dirs);
            }
            self.urls.clear();
            for (dir, prefix) in &self.outside {
                add_beside(&mut project, &mut self.urls, dir, prefix);
                dirs.push(dir.clone());
            }
            self.stamps = dirs
                .into_iter()
                .map(|d| {
                    let at = modified(&d);
                    (d, at)
                })
                .collect();
            self.made.clear();
            self.failed.clear();
            self.project = Some(project);
        }
        Ok(self.project.as_mut().expect("read above"))
    }

    /// The directory of a text placed in one outside `dirs`, read once.
    fn place(&mut self, key: &str, url: &str) {
        let Some(dir) = ljos_engine::url_path(url).and_then(|p| p.parent().map(Path::to_owned))
        else {
            return;
        };
        let Some((prefix, _)) = key.rsplit_once('/') else {
            return;
        };
        let inside = self.dirs.iter().any(|d| {
            let d = d.canonicalize().unwrap_or_else(|_| d.clone());
            dir.starts_with(d)
        });
        if inside || !dir.is_dir() || self.outside.iter().any(|(d, _)| *d == dir) {
            return;
        }
        if let Some(project) = &mut self.project {
            add_beside(project, &mut self.urls, &dir, prefix);
        }
        let at = modified(&dir);
        self.stamps.push((dir.clone(), at));
        self.outside.push((dir, prefix.to_owned()));
        self.made.clear();
    }

    fn compile(&mut self, text: QmlText<'_>) -> Result<CompiledFile, Vec<QmlTextError>> {
        self.fresh().map_err(plain)?;
        self.place(text.key, text.url);
        let project = self.project.as_mut().expect("read above");
        let errors = |diagnostics: &[ljos_compiler::Diagnostic]| {
            diagnostics
                .iter()
                .filter(|d| d.severity == ljos_compiler::Severity::Error)
                .map(|d| QmlTextError {
                    line: d.span.line,
                    column: d.span.column,
                    message: d.message.clone(),
                })
                .collect()
        };
        match compile_text(project, text.key, text.text) {
            Ok(compiled) if !compiled.has_errors() => Ok(compiled.file),
            Ok(compiled) => Err(errors(&compiled.diagnostics)),
            Err(diagnostics) => Err(errors(&diagnostics)),
        }
    }

    fn load(&mut self, key: &str) -> Option<Arc<CompiledFile>> {
        if let Some(file) = self.made.get(key) {
            return Some(Arc::clone(file));
        }
        let project = self.project.as_ref()?;
        project.source(key)?;
        match compile_file(project, key) {
            Ok(compiled) if !compiled.has_errors() => {
                let file = Arc::new(compiled.file);
                self.made.insert(key.to_owned(), Arc::clone(&file));
                Some(file)
            }
            Ok(compiled) => {
                let why = compiled.diagnostics.first().map(ToString::to_string);
                self.failed.insert(key.to_owned(), why.unwrap_or_default());
                None
            }
            Err(diagnostics) => {
                let why = diagnostics.first().map(ToString::to_string);
                self.failed.insert(key.to_owned(), why.unwrap_or_default());
                None
            }
        }
    }
}

/// The QML files and scripts of `dir` in `project`, keyed under `prefix`
/// as a file of the directory is.
fn add_beside(
    project: &mut Project<EngineOracle>,
    urls: &mut HashMap<String, String>,
    dir: &Path,
    prefix: &str,
) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Some(stem) = name.strip_suffix(".qml") {
            let key = format!("{prefix}/{stem}");
            if project.source(&key).is_none() {
                project.add(&key, text);
                urls.insert(key, ljos_engine::file_url(&path));
            }
        } else if name.ends_with(".js") || name.ends_with(".mjs") {
            project.add_script(&format!("{prefix}/{name}"), text);
        }
    }
}

/// The application's own source, then what the texts read.
struct Fallback {
    source: Rc<dyn ComponentSource>,
    state: Rc<RefCell<State>>,
}

impl ComponentSource for Fallback {
    fn load(&self, key: &str) -> Option<Arc<CompiledFile>> {
        self.source
            .load(key)
            .or_else(|| self.state.borrow_mut().load(key))
    }

    fn contains(&self, key: &str) -> bool {
        self.source.contains(key) || self.state.borrow_mut().load(key).is_some()
    }

    fn failure(&self, key: &str) -> Option<String> {
        self.source
            .failure(key)
            .or_else(|| self.state.borrow().failed.get(key).cloned())
    }

    fn url(&self, key: &str) -> String {
        let state = self.state.borrow();
        if self.source.load(key).is_none() {
            if let Some(url) = state.urls.get(key) {
                return url.clone();
            }
            if let Some(path) = state.project.as_ref().and_then(|p| p.path(key)) {
                return ljos_engine::file_url(path);
            }
        }
        self.source.url(key)
    }

    fn key(&self, url: &str) -> String {
        self.source.key(url)
    }

    fn bytecode(&self, key: &str) -> Option<Bytecode> {
        self.source.bytecode(key)
    }
}
