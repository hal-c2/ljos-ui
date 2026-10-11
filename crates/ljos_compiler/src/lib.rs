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
pub mod tier1;

pub use compile::compile_file;
pub use dump::dump;
pub use project::Project;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: SourceSpan,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            span,
        }
    }

    pub fn warning(message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            severity: Severity::Warning,
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
    pub result: Result<Compiled, Vec<Diagnostic>>,
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
    let project = Project::load(dir, oracle)?;
    Ok(project
        .files()
        .map(|key| FileResult {
            key: key.to_string(),
            result: compile_file(&project, key),
        })
        .collect())
}
