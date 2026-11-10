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
use ljos_engine::{
    ComponentSource, Engine, MapSource, QmlText, QmlTextCompiler, QmlTextError, WeakEngine,
};
use ljos_ir::CompiledFile;
use ljos_js::Bytecode;
use qml_front::TypeOracle;

use crate::compile::EngineOracle;

/// The compiler and the component source of one application, over the
/// same files.
pub(crate) struct DiskTexts {
    state: Rc<RefCell<State>>,
}

/// The native types a text is compiled against.
enum Oracle {
    Ready(EngineOracle),
    /// Those of the engine when its first text is made. Describing every
    /// registered type loads each style library's files and builds their
    /// types, megabytes a program that makes no text never needs. Weak: the
    /// engine holds the compiler.
    Later(WeakEngine),
}

struct State {
    dirs: Vec<PathBuf>,
    oracle: Oracle,
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
        Self::with_oracle(dirs, Oracle::Ready(oracle))
    }

    /// Against the types `engine` has when its first text is made.
    pub(crate) fn of_engine(dirs: &[&Path], engine: &Engine) -> Self {
        Self::with_oracle(dirs, Oracle::Later(engine.downgrade()))
    }

    fn with_oracle(dirs: &[&Path], oracle: Oracle) -> Self {
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
        let compiler: QmlTextCompiler = Rc::new(move |text: QmlText<'_>| {
            // Described with the state let go: describing a type loads
            // files, and nothing it reaches may find the state held.
            let later = match &state.borrow().oracle {
                Oracle::Later(engine) => Some(engine.clone()),
                Oracle::Ready(_) => None,
            };
            if let Some(engine) = later {
                let engine = engine
                    .upgrade()
                    .ok_or_else(|| plain("the engine is gone".to_owned()))?;
                let oracle = EngineOracle::from_engine(&engine);
                state.borrow_mut().oracle = Oracle::Ready(oracle);
            }
            state.borrow_mut().compile(text)
        });
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

pub(crate) fn plain(message: String) -> Vec<QmlTextError> {
    vec![QmlTextError {
        line: 0,
        column: 0,
        message,
    }]
}

/// `text` compiled in `project`, or the errors found in it.
pub(crate) fn compile_qml_text<O: TypeOracle>(
    project: &mut Project<O>,
    text: QmlText<'_>,
) -> Result<CompiledFile, Vec<QmlTextError>> {
    match compile_text(project, text.key, text.text) {
        Ok(compiled) if !compiled.has_errors() => Ok(compiled.file),
        Ok(compiled) => Err(text_errors(compiled.all_diagnostics())),
        Err(diagnostics) => Err(text_errors(diagnostics.iter().map(|d| (None, d)))),
    }
}

impl State {
    fn stale(&self) -> bool {
        self.project.is_none() || self.stamps.iter().any(|(dir, at)| modified(dir) != *at)
    }

    /// The project, read again when a directory changed since it was.
    fn fresh(&mut self) -> Result<&mut Project<EngineOracle>, String> {
        if self.stale() {
            let roots: Vec<&Path> = self.dirs.iter().map(PathBuf::as_path).collect();
            let Oracle::Ready(oracle) = &self.oracle else {
                return Err("the types of the engine were not read".to_owned());
            };
            let mut project =
                Project::load_roots(&roots, oracle.clone()).map_err(|e| e.to_string())?;
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
        compile_qml_text(self.project.as_mut().expect("read above"), text)
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
                // Its own error, or else that of a module it reaches.
                let why = compiled
                    .diagnostics
                    .iter()
                    .find(|d| d.severity == ljos_compiler::Severity::Error)
                    .map_or_else(|| compiled.failure_reason(), ToString::to_string);
                self.failed.insert(key.to_owned(), why);
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
    project.add_place(dir, prefix);
    let names: Vec<String> = read
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    for name in &names {
        let path = dir.join(name);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Some(stem) = name.strip_suffix(".qml") {
            let key = format!("{prefix}/{stem}");
            if project.source(&key).is_none() {
                project.add(&key, text);
                urls.insert(key, ljos_engine::file_url(&path));
            }
        } else if qml_front::project::is_script(name) {
            project.add_script(&format!("{prefix}/{name}"), text);
        }
    }
    // As the project loaders read them: a `.d.ts` with no JavaScript file
    // of its own beside it types nothing here either.
    for name in ljos_compiler::declared_beside(&names) {
        if let Ok(text) = std::fs::read_to_string(dir.join(name)) {
            project.add_declarations(&format!("{prefix}/{name}"), text);
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

/// The errors of a text, and of the modules it reaches (`Some` with the
/// module's key), as a text's errors ([`ljos_compiler::text_error`]).
pub(crate) fn text_errors<'a>(
    diagnostics: impl Iterator<Item = (Option<&'a str>, &'a ljos_compiler::Diagnostic)>,
) -> Vec<QmlTextError> {
    diagnostics
        .filter(|(_, d)| d.severity == ljos_compiler::Severity::Error)
        .map(|(module, d)| {
            let (line, column, message) = ljos_compiler::text_error(module, d);
            QmlTextError {
                line,
                column,
                message,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_types_of_the_engine_are_described_at_the_first_text_not_at_the_start() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            dir.path().join("Main.qml"),
            "import QtQuick\nItem {\n    property string made: \"\"\n    \
             function make(text) { made = Qt.createQmlObject(text, this, \"inline.qml\").objectName }\n}\n",
        )
        .expect("write");
        let engine = crate::engine();
        let source = crate::compile::compile_dirs(
            &[dir.path()],
            Some(&engine),
            crate::compile::OnError::Fail,
        )
        .expect("compile");
        engine.set_component_source(source);
        let texts = DiskTexts::of_engine(&[dir.path()], &engine);
        let state = Rc::clone(&texts.state);
        texts.install_over(&engine);
        let root = engine
            .create_from_url("Main", None, Vec::new())
            .expect("Main");
        assert!(matches!(state.borrow().oracle, Oracle::Later(_)));

        let text = "import QtQuick\nItem { objectName: \"made\" }";
        engine
            .invoke_by_name(root, "make", &[ljos_engine::Value::from(text)])
            .expect("make");
        assert_eq!(
            engine.read_by_name(root, "made").to_js_string().as_ref(),
            "made"
        );
        assert!(matches!(state.borrow().oracle, Oracle::Ready(_)));
    }

    #[test]
    fn a_directory_beside_a_text_reads_only_the_declarations_of_its_javascript() {
        let dir = tempfile::tempdir().expect("temp dir");
        for (name, text) in [
            ("calc.js", "function add(a, b) { return a + b }\n"),
            (
                "calc.d.ts",
                "export declare function add(a: number, b: number): number;\n",
            ),
            ("lone.d.ts", "export declare const n: number;\n"),
            ("calc.d.mts", "export declare const m: number;\n"),
        ] {
            std::fs::write(dir.path().join(name), text).expect("write");
        }
        let mut project = Project::new(EngineOracle::from_engine(&Engine::new()));
        let mut urls = HashMap::new();
        add_beside(&mut project, &mut urls, dir.path(), "outside");
        let mut read: Vec<&str> = project.declarations().map(|(path, _)| path).collect();
        read.sort_unstable();
        assert_eq!(read, ["outside/calc.d.ts"]);
    }
}
