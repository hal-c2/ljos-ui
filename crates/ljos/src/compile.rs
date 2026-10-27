//! Compiling QML against the table of types and an Engine's native
//! types (the [`EngineOracle`]): what [`crate::dev`] does at start-up and
//! [`crate::build`] does in a build script. Either feature links the
//! parser.

use std::fmt;
use std::path::{Component, Path, PathBuf};

#[cfg(feature = "dev-qml")]
use ljos_engine::QmlTextCompiler;
use ljos_engine::{Engine, MapSource};
use qml_front::{QtTypeOracle, TypeOracle};

pub use crate::oracle::{EngineOracle, EngineType, NativeTypeError};

/// Why a compile of QML directories gave no component source.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CompileError {
    /// A directory or a file under it could not be read.
    #[error("cannot read {}: {source}", path.display())]
    Unreadable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// With [`OnError::Fail`], a file had an error: the directories
    /// compiled, and every error, in the order [`Compiled::diagnostics`]
    /// has them.
    #[error("{} did not compile:\n{}", named(.dirs), lines(.errors))]
    Failed {
        dirs: Vec<PathBuf>,
        errors: Vec<Diagnostic>,
    },
}

impl From<ljos_compiler::Error> for CompileError {
    fn from(e: ljos_compiler::Error) -> Self {
        match e {
            ljos_compiler::Error::Io { path, source } => CompileError::Unreadable { path, source },
        }
    }
}

fn named(dirs: &[PathBuf]) -> String {
    dirs.iter()
        .map(|d| d.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn lines(errors: &[Diagnostic]) -> String {
    errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

/// What a compile does with a file that has an error.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OnError {
    /// The file is noted as one that did not compile
    /// ([`MapSource::insert_failure`]) and the rest of the directory
    /// is used: making that component fails with the compiler's
    /// message, as a QML engine fails a component only where it is
    /// made. A file nothing makes does not stop the application.
    #[default]
    Record,
    /// Any file with an error fails the whole compile, one error a
    /// line: for a build step or CI that wants every file clean.
    Fail,
}

/// What a compile of QML directories found ([`compile_quiet`]): the
/// files as a component source, and every diagnostic of every file.
#[derive(Debug)]
pub struct Compiled {
    /// The compiled files; each file with an error is recorded as one
    /// that did not compile ([`MapSource::failures`]).
    pub source: MapSource,
    /// In key order, and in the order the compiler found them in a
    /// file.
    pub diagnostics: Vec<Diagnostic>,
}

impl Compiled {
    /// Whether a diagnostic is an error: some file did not compile.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

/// How bad a [`Diagnostic`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Severity {
    /// The file did not compile: making it fails.
    Error,
    /// The file compiled, to something only the running program can
    /// settle (a name looked up when it runs, say).
    Warning,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        })
    }
}

/// One thing a compile found in a file. Shown as a compiler at a
/// terminal says it, `path:line:column: severity: message`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Diagnostic {
    /// The file, as found under the directory it was compiled from.
    pub path: PathBuf,
    /// The key the file is known by (`Main`, `controls/Button`).
    pub key: String,
    /// 1-based, in characters.
    pub line: u32,
    pub column: u32,
    pub severity: Severity,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}: {}",
            self.path.display(),
            self.line,
            self.column,
            self.severity,
            self.message
        )
    }
}

/// Compiles every file under `dir` into a component source, against
/// the table of types alone: an application's native types are not
/// in the table (ARCHITECTURE.md D1), so a file that imports one is
/// recorded as not compiling (an unknown type). Prints the
/// diagnostics as a developer at a terminal expects; a file with an
/// error is recorded, not fatal ([`OnError::Record`]).
#[cfg(feature = "dev-qml")]
pub fn compile_dir(dir: &Path) -> Result<MapSource, CompileError> {
    compile(dir, None, OnError::Record)
}

/// Compiles every file under `dir` into a component source, against
/// the table of types plus the native types registered on `engine`
/// that the table does not describe ([`EngineOracle::from_engine`]:
/// a type the table knows stays the table's, natives fill the
/// gaps). A file may `import` the application's own module and
/// create objects of its types. Same diagnostics and recording as
/// [`compile_dir`].
#[cfg(feature = "dev-qml")]
pub fn compile_dir_with(dir: &Path, engine: &Engine) -> Result<MapSource, CompileError> {
    compile(dir, Some(engine), OnError::Record)
}

/// Compiles every file under `dir`, against the table of types and,
/// given an `engine`, its native types as [`compile_dir_with`] does;
/// `on_error` says whether a file with an error is recorded or fails
/// the compile. `Err` also when `dir` cannot be read.
#[cfg(feature = "dev-qml")]
pub fn compile(
    dir: &Path,
    engine: Option<&Engine>,
    on_error: OnError,
) -> Result<MapSource, CompileError> {
    compile_dirs(&[dir], engine, on_error)
}

/// [`compile`] over several QML roots, an application's import path:
/// each root's files are keyed by their path from that root (the
/// entry of `app/Main.qml` under the root `app` is `Main`), and a file
/// of one root may import a module whose `qmldir` is in another.
/// Where two roots have a file of the same key the earlier root's is
/// used, as the first import path that has a module is.
///
/// Each diagnostic is printed to stderr as one line
/// (`path:line:column: severity: message`); [`compile_quiet`] hands
/// them back instead.
pub fn compile_dirs(
    dirs: &[&Path],
    engine: Option<&Engine>,
    on_error: OnError,
) -> Result<MapSource, CompileError> {
    match engine {
        Some(engine) => compile_with_oracle(dirs, EngineOracle::from_engine(engine), on_error),
        None => compile_with_oracle(dirs, QtTypeOracle, on_error),
    }
}

/// [`compile_dirs`] printing nothing and failing no file: what the
/// compile found comes back as values, for an application to show
/// its own way, count, or hold to none. Each file has its own URL,
/// `file://` and its absolute path, so `Qt.resolvedUrl` and every
/// relative address in it resolve beside the file, as a QML file
/// loaded from disk does. `Err` only when a directory cannot be read.
#[cfg(feature = "dev-qml")]
pub fn compile_quiet(dirs: &[&Path], engine: Option<&Engine>) -> Result<Compiled, CompileError> {
    match engine {
        Some(engine) => compile_quiet_with_oracle(dirs, EngineOracle::from_engine(engine)),
        None => compile_quiet_with_oracle(dirs, QtTypeOracle),
    }
}

/// Gives `Qt.createQmlObject` on `engine` a compiler (development only,
/// the `dev-qml` feature): a text is compiled as a file at the place the
/// script names for it, against the files under `dirs` and the native
/// types registered on `engine` now, so it finds the types beside that
/// place as a file there would. [`App::qml_dirs`](crate::App::qml_dirs)
/// does this itself; an engine whose components came from
/// [`compile_dirs`] or [`compile_quiet`] (a harness, a
/// [`TestWindow`](crate::testing)) is given one with this call, after
/// its components are set: it answers what they lack, as under
/// `qml_dirs`, so a text or a file placed outside `dirs` finds the files
/// beside it, and a file added since the start is found.
///
/// The directories are read when the first text is made, not before:
/// most programs never make an object from text. A program without
/// the feature has no compiler, and the call is an error its script can
/// catch.
#[cfg(feature = "dev-qml")]
pub fn install_text_compiler(engine: &Engine, dirs: &[&Path]) {
    crate::texts::DiskTexts::new(dirs, EngineOracle::from_engine(engine)).install_over(engine);
}

/// A compiler for a host that installs it itself
/// (`Engine::set_qml_text_compiler`); without an `engine` against the
/// table of types alone. It reads `dirs` at its first text and gives the
/// engine no source, so unlike [`install_text_compiler`] it finds no file
/// outside them.
#[cfg(feature = "dev-qml")]
#[must_use]
pub fn text_compiler(dirs: &[&Path], engine: Option<&Engine>) -> QmlTextCompiler {
    match engine {
        Some(engine) => text_compiler_with_oracle(dirs, EngineOracle::from_engine(engine)),
        None => text_compiler_with_oracle(dirs, QtTypeOracle),
    }
}

#[cfg(feature = "dev-qml")]
pub(crate) fn text_compiler_with_oracle<O: TypeOracle + Clone + 'static>(
    dirs: &[&Path],
    oracle: O,
) -> QmlTextCompiler {
    use std::cell::RefCell;

    use ljos_compiler::Project;
    use ljos_engine::QmlText;

    use crate::texts::{compile_qml_text, plain};

    let dirs: Vec<PathBuf> = dirs.iter().map(|d| d.to_path_buf()).collect();
    let project: RefCell<Option<Project<O>>> = RefCell::new(None);
    std::rc::Rc::new(move |text: QmlText<'_>| {
        let mut project = project.borrow_mut();
        let project = match &mut *project {
            Some(project) => project,
            empty => {
                let roots: Vec<&Path> = dirs.iter().map(PathBuf::as_path).collect();
                let loaded = Project::load_roots(&roots, oracle.clone())
                    .map_err(|e| plain(e.to_string()))?;
                empty.insert(loaded)
            }
        };
        compile_qml_text(project, text)
    })
}

pub(crate) fn compile_quiet_with_oracle<O: TypeOracle>(
    dirs: &[&Path],
    oracle: O,
) -> Result<Compiled, CompileError> {
    let mut source = MapSource::new();
    let mut found = Vec::new();
    let files = ljos_compiler::compile_roots_with(dirs, oracle)?;
    for r in files {
        let path = r.path.clone().unwrap_or_else(|| PathBuf::from(&r.key));
        let diagnostics = match &r.result {
            Ok(c) => &c.diagnostics,
            Err(ds) => ds,
        };
        found.extend(diagnostics.iter().map(|d| Diagnostic {
            path: path.clone(),
            key: r.key.clone(),
            line: d.span.line,
            column: d.span.column,
            severity: match d.severity {
                ljos_compiler::Severity::Error => Severity::Error,
                ljos_compiler::Severity::Warning => Severity::Warning,
            },
            message: d.message.clone(),
        }));
        if let Some(path) = &r.path {
            source.insert_url(r.key.clone(), absolute_url(path));
        }
        match r.result {
            Ok(c) if !c.has_errors() => source.insert(r.key, c.file),
            // Compiled only as far as it could: not a file to run.
            Ok(c) => source.insert_failure(r.key, ljos_compiler::failure_reason(&c.diagnostics)),
            Err(ds) => source.insert_failure(r.key, ljos_compiler::failure_reason(&ds)),
        }
    }
    Ok(Compiled {
        source,
        diagnostics: found,
    })
}

/// [`compile_quiet_with_oracle`], printing each diagnostic, and with
/// [`OnError::Fail`] failing on any error.
pub(crate) fn compile_with_oracle<O: TypeOracle>(
    dirs: &[&Path],
    oracle: O,
    on_error: OnError,
) -> Result<MapSource, CompileError> {
    let compiled = compile_quiet_with_oracle(dirs, oracle)?;
    for d in &compiled.diagnostics {
        eprintln!("{d}");
    }
    checked_source(dirs, compiled, on_error)
}

/// `compiled`'s source, or with [`OnError::Fail`] and an error, every
/// error a line.
pub(crate) fn checked_source(
    dirs: &[&Path],
    compiled: Compiled,
    on_error: OnError,
) -> Result<MapSource, CompileError> {
    if on_error == OnError::Fail && compiled.has_errors() {
        return Err(CompileError::Failed {
            dirs: dirs.iter().map(|d| d.to_path_buf()).collect(),
            errors: compiled
                .diagnostics
                .into_iter()
                .filter(|d| d.severity == Severity::Error)
                .collect(),
        });
    }
    Ok(compiled.source)
}

/// The `file://` URL of `path` ([`ljos_engine::file_url`]), made
/// absolute against the working directory and without `.` and `..`, so
/// that it is the same text the engine reaches when it resolves a
/// relative address beside another file.
fn absolute_url(path: &Path) -> String {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    let mut clean = PathBuf::new();
    for part in absolute.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                clean.pop();
            }
            part => clean.push(part),
        }
    }
    ljos_engine::file_url(&clean)
}
