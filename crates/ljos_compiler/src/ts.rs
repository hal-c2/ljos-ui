//! Ljós's API and its files as TypeScript: the declarations of the API
//! ([`dts`]), each file in it as a module ([`extract`]), and TypeScript run
//! on them ([`tsc`]), which is the checker's oracle: a file TypeScript
//! accepts is one the checker accepts (docs/DIALECT.md M4).

use std::path::{Path, PathBuf};

use qml_front::QtTypeOracle;

use crate::Project;

pub mod dts;
pub mod extract;
pub mod tsc;

pub use dts::Api;
pub use extract::{Extracted, extract};
pub use tsc::Checker;

/// The name the declarations are written under, beside the modules.
pub const DECLARATIONS: &str = "ljos.d.ts";

#[derive(Debug, thiserror::Error)]
pub enum TsError {
    #[error("the table of types has no module {0}")]
    NoApi(&'static str),
    #[error("cannot write {}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error(transparent)]
    Project(#[from] crate::Error),
    #[error("cannot start {}: {source}", program.display())]
    Start {
        program: PathBuf,
        source: std::io::Error,
    },
    #[error("{} failed and said nothing of the files:\n{output}", program.display())]
    Failed { program: PathBuf, output: String },
}

/// What TypeScript says of a line of a QML file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The file's key in the project.
    pub key: String,
    pub line: u32,
    pub column: u32,
    /// `TS2322`.
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.qml:{}:{}: error: {} [{}]",
            self.key, self.line, self.column, self.message, self.code
        )
    }
}

/// What a check by TypeScript came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The machine has no TypeScript.
    Skipped,
    /// What it found in the files in Ljós's API, by file and line; the keys
    /// of the files it was given.
    Checked {
        files: Vec<String>,
        findings: Vec<Finding>,
    },
}

/// Gives each file of the project in `dir` that is in Ljós's API to
/// TypeScript, with `checker` or the one [`Checker::find`] finds.
pub fn check_dir(dir: &Path, checker: Option<Checker>) -> Result<Outcome, TsError> {
    let project = Project::load_roots(&[dir], QtTypeOracle)?;
    check_project(&project, checker)
}

pub fn check_project(
    project: &Project<QtTypeOracle>,
    checker: Option<Checker>,
) -> Result<Outcome, TsError> {
    let Some(checker) = checker.or_else(Checker::find) else {
        return Ok(Outcome::Skipped);
    };
    let api = Api::new().ok_or(TsError::NoApi(dts::API))?;
    let scratch = Scratch::new()?;
    let declarations = scratch.0.join(DECLARATIONS);
    std::fs::write(&declarations, api.declarations()).map_err(|source| TsError::Io {
        path: declarations.clone(),
        source,
    })?;
    let mut modules = Vec::new();
    let mut keys: Vec<&str> = project.files().collect();
    keys.sort_unstable();
    for key in keys {
        let Some(extracted) = extract(project, &api, key, &format!("./{DECLARATIONS}")) else {
            continue;
        };
        let name = format!("f{}.ts", modules.len());
        let path = scratch.0.join(&name);
        std::fs::write(&path, &extracted.text).map_err(|source| TsError::Io { path, source })?;
        modules.push((name, key.to_owned(), extracted));
    }
    if modules.is_empty() {
        return Ok(Outcome::Checked {
            files: Vec::new(),
            findings: Vec::new(),
        });
    }
    let names: Vec<String> = modules.iter().map(|(name, _, _)| name.clone()).collect();
    let said = checker.check(&scratch.0, &names)?;
    let mut findings: Vec<Finding> = said
        .into_iter()
        .filter_map(|said| {
            let (_, key, extracted) = modules.iter().find(|(name, _, _)| *name == said.file)?;
            let (line, column) = extracted.position(said.line, said.column).unwrap_or((1, 1));
            Some(Finding {
                key: key.clone(),
                line,
                column,
                code: said.code,
                message: said.message,
            })
        })
        .collect();
    findings.sort_by(|a, b| (&a.key, a.line, a.column).cmp(&(&b.key, b.line, b.column)));
    findings.dedup();
    Ok(Outcome::Checked {
        files: modules.into_iter().map(|(_, key, _)| key).collect(),
        findings,
    })
}

/// A directory of its own under the system's temporary one, gone when the
/// check is over.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self, TsError> {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ljos-tsc-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).map_err(|source| TsError::Io {
            path: dir.clone(),
            source,
        })?;
        Ok(Self(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
