//! QML to Ljós IR: the compiler a build runs, so a shipped program never
//! parses QML (ARCHITECTURE.md §5.4).
//!
//! A file becomes a [`ljos_ir::CompiledFile`]: its objects, the values that
//! never change as literals, and every other script as a function of one
//! JavaScript module per file, whose parameters are what the script's free
//! names were resolved to.

use std::{
    fmt,
    path::{Path, PathBuf},
};

use ljos_ir::{CompiledFile, SourceSpan};
use qml_front::{QtTypeOracle, TypeOracle};

pub mod baseline;
mod compile;
mod dump;
pub mod i18n;
mod js;
mod oracle;
mod project;
pub mod reload;
pub mod reload_lint;
mod script;
pub mod tier1;

pub use compile::compile_file;
pub use dump::dump;
pub use project::{Project, controls_style, resources};
pub use reload::{CompileFault, FileSet, ReloadSession, diff, diff_file};

/// A file's compiled form, with what was wrong with it. A file with errors
/// still compiles as far as it can.
#[derive(Debug, Clone)]
pub struct Compiled {
    pub file: CompiledFile,
    pub diagnostics: Vec<Diagnostic>,
}

impl Compiled {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    /// Compiled, but to something only the running program can settle: a
    /// name that is not in sight, say.
    Warning,
}

/// The kind of a [`Diagnostic`], by which a program tells kinds apart
/// without reading the message, which may change. The front end's kinds
/// keep the front end's names ([`qml_front::Code`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Code {
    /// The parser rejected part of the file.
    Syntax,
    /// An import names a module nothing knows.
    UnknownModule,
    /// A type is not provided by anything the file imports.
    UnknownType,
    /// A binding names a property the type does not have.
    UnknownProperty,
    /// A handler is of a signal the type does not have.
    UnknownSignal,
    /// Two objects of one component share an `id`.
    DuplicateId,
    /// A `property alias` points at an id the component does not have.
    UnknownAliasTarget,
    /// A free name in a script that nothing in reach has.
    UnresolvedName,
    /// Anything else the front end found.
    Semantic,
    /// The file asked for is not one of the project's.
    NotInProject,
    /// An object of a type nothing may make (a singleton's).
    NotCreatable,
    /// `id.member`, where what the id names has no such member.
    UnknownMember,
    /// A value written for a property it cannot hold: a number for a
    /// string, a name that is no colour, a key of no enumeration.
    InvalidAssignment,
    /// A script in the file does not parse as JavaScript.
    ScriptSyntax,
    /// A script the file imports cannot be used.
    ScriptUnavailable,
    /// Objects a reload can tell apart only by their order.
    ReloadIdentity,
}

impl Code {
    /// The stable, hyphenated name: `invalid-assignment`.
    pub fn as_str(self) -> &'static str {
        match self {
            Code::Syntax => "syntax",
            Code::UnknownModule => "unknown-module",
            Code::UnknownType => "unknown-type",
            Code::UnknownProperty => "unknown-property",
            Code::UnknownSignal => "unknown-signal",
            Code::DuplicateId => "duplicate-id",
            Code::UnknownAliasTarget => "unknown-alias-target",
            Code::UnresolvedName => "unresolved-name",
            Code::Semantic => "semantic",
            Code::NotInProject => "not-in-project",
            Code::NotCreatable => "not-creatable",
            Code::UnknownMember => "unknown-member",
            Code::InvalidAssignment => "invalid-assignment",
            Code::ScriptSyntax => "script-syntax",
            Code::ScriptUnavailable => "script-unavailable",
            Code::ReloadIdentity => "reload-identity",
        }
    }
}

impl From<qml_front::Code> for Code {
    fn from(code: qml_front::Code) -> Self {
        match code {
            qml_front::Code::Syntax => Code::Syntax,
            qml_front::Code::UnknownModule => Code::UnknownModule,
            qml_front::Code::UnknownType => Code::UnknownType,
            qml_front::Code::UnknownProperty => Code::UnknownProperty,
            qml_front::Code::UnknownSignal => Code::UnknownSignal,
            qml_front::Code::DuplicateId => Code::DuplicateId,
            qml_front::Code::UnknownAliasTarget => Code::UnknownAliasTarget,
            qml_front::Code::UnresolvedName => Code::UnresolvedName,
            // A kind the front end has that this does not name yet.
            _ => Code::Semantic,
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Code,
    pub message: String,
    pub span: SourceSpan,
}

impl Diagnostic {
    pub fn error(code: Code, message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message: message.into(),
            span,
        }
    }

    pub fn warning(code: Code, message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            severity: Severity::Warning,
            code,
            message: message.into(),
            span,
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(
            f,
            "{}:{}: {severity}: {}",
            self.span.line, self.span.column, self.message
        )
    }
}

/// Why a file with these diagnostics cannot be made, as a program says it
/// to whatever makes an object of the file: the first error's place and
/// message (`3:5: message`), which a loader puts after the file's URL.
#[must_use]
pub fn failure_reason(diagnostics: &[Diagnostic]) -> String {
    match diagnostics.iter().find(|d| d.severity == Severity::Error) {
        Some(d) => format!("{}:{}: {}", d.span.line, d.span.column, d.message),
        None => "the file does not compile".to_owned(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read {}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// One file of a project compiled, or why it was not.
#[derive(Debug)]
pub struct FileResult {
    pub key: String,
    /// The file it was read from.
    pub path: Option<PathBuf>,
    pub result: Result<Compiled, Vec<Diagnostic>>,
}

impl FileResult {
    /// The file as a program can run it, or why it cannot
    /// ([`failure_reason`]). A file with an error compiled only as far as
    /// it could, so it is not one to run.
    pub fn usable(&self) -> Result<&Compiled, String> {
        match &self.result {
            Ok(c) if !c.has_errors() => Ok(c),
            Ok(c) => Err(failure_reason(&c.diagnostics)),
            Err(ds) => Err(failure_reason(ds)),
        }
    }
}

/// Compiles `text` as the file `key` of the project would be compiled: it
/// sees the types a file of that key sees. For QML a running program holds
/// as text (`Qt.createQmlObject`), which only a development host can make
/// an object of. The project is left as it was.
pub fn compile_text<O: TypeOracle>(
    project: &mut Project<O>,
    key: &str,
    text: &str,
) -> Result<Compiled, Vec<Diagnostic>> {
    let before = project.source(key).map(str::to_owned);
    project.add(key, text);
    let compiled = compile_file(project, key);
    match before {
        Some(source) => project.add(key, source),
        None => project.remove(key),
    }
    compiled
}

/// Compiles every QML file under `dir` against Qt's types.
pub fn compile_project(dir: &Path) -> Result<Vec<FileResult>, Error> {
    compile_project_with(dir, QtTypeOracle)
}

/// Compiles every QML file under `dir` against the types `oracle` knows.
pub fn compile_project_with<O: TypeOracle>(
    dir: &Path,
    oracle: O,
) -> Result<Vec<FileResult>, Error> {
    compile_roots_with(&[dir], oracle)
}

/// Compiles every QML file under each of `roots` against the types
/// `oracle` knows, each keyed from its own root ([`Project::load_roots`]).
pub fn compile_roots_with<O: TypeOracle>(
    roots: &[&Path],
    oracle: O,
) -> Result<Vec<FileResult>, Error> {
    let project = Project::load_roots(roots, oracle)?;
    Ok(project
        .files()
        .map(|key| FileResult {
            key: key.to_string(),
            path: project.path(key).map(Path::to_owned),
            result: compile_file(&project, key),
        })
        .collect())
}
