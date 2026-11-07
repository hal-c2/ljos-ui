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

/// The directory, beside the modules, the project's TypeScript modules are
/// written under as they are laid out in the project, so that what one
/// imports from another is found.
pub const MODULES: &str = "m";

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
    /// TypeScript cannot read the declarations, or found an error in a file
    /// that is not one of the modules: nothing it said of the files can be
    /// trusted, so the check fails rather than passing.
    #[error("TypeScript found errors outside the files it was given:\n{said}")]
    Declarations { said: String },
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
        // A TypeScript module's key is its file's name; a QML file's is not.
        let suffix = if crate::script::module::is_module(&self.key)
            || crate::script::module::is_declarations(&self.key)
        {
            ""
        } else {
            ".qml"
        };
        write!(
            f,
            "{}{suffix}:{}:{}: error: {} [{}]",
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
    let declarations = scratch.path().join(DECLARATIONS);
    std::fs::write(&declarations, api.declarations()).map_err(|source| TsError::Io {
        path: declarations.clone(),
        source,
    })?;
    let mut scripts: Vec<String> = Vec::new();
    for (key, text) in project.scripts() {
        if !crate::script::module::is_module(key) || !within(key) {
            continue;
        }
        let path = scratch.path().join(MODULES).join(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| TsError::Io {
                path: parent.to_owned(),
                source,
            })?;
        }
        std::fs::write(&path, text).map_err(|source| TsError::Io { path, source })?;
        scripts.push(key.to_owned());
    }
    // A JavaScript file with declarations beside it is given with them,
    // and is told where they are (`@ts-self-types`, which `deno` reads; `tsc`
    // finds them beside it itself). One without is not given: what reaches
    // it is the checker's to say (`ts-boundary`).
    for (key, text) in project.declarations() {
        let Some(script) = project
            .scripts()
            .map(|(script, _)| script)
            .find(|script| crate::script::module::declarations_of(script).as_deref() == Some(key))
        else {
            continue;
        };
        if !within(key) {
            continue;
        }
        let name = key.rsplit('/').next().unwrap_or(key);
        let told = format!("// @ts-self-types=\"./{name}\"\n");
        for (key, text) in [(key, text.to_owned()), (script, told)] {
            let path = scratch.path().join(MODULES).join(key);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|source| TsError::Io {
                    path: parent.to_owned(),
                    source,
                })?;
            }
            std::fs::write(&path, text).map_err(|source| TsError::Io { path, source })?;
        }
        scripts.push(key.to_owned());
    }
    let mut modules = Vec::new();
    let mut keys: Vec<&str> = project.files().collect();
    keys.sort_unstable();
    for key in keys {
        let Some(extracted) = extract(
            project,
            &api,
            key,
            &format!("./{DECLARATIONS}"),
            &format!("./{MODULES}"),
        ) else {
            continue;
        };
        let name = format!("f{}.ts", modules.len());
        let path = scratch.path().join(&name);
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
    let said = checker.check(scratch.path(), &names)?;
    let mut findings: Vec<Finding> = Vec::new();
    let mut elsewhere: Vec<String> = Vec::new();
    for said in said {
        // A module is its own text, so what TypeScript says of it is at its
        // own lines.
        if let Some(key) = said
            .file
            .strip_prefix(MODULES)
            .and_then(|rest| rest.strip_prefix('/'))
            .filter(|key| scripts.iter().any(|s| s == key))
        {
            findings.push(Finding {
                key: key.to_owned(),
                line: said.line,
                column: said.column,
                code: said.code,
                message: said.message,
            });
            continue;
        }
        let Some((_, key, extracted)) = modules.iter().find(|(name, _, _)| *name == said.file)
        else {
            elsewhere.push(format!(
                "{}:{}:{}: {} [{}]",
                said.file, said.line, said.column, said.message, said.code
            ));
            continue;
        };
        let (line, column) = extracted.position(said.line, said.column).unwrap_or((1, 1));
        findings.push(Finding {
            key: key.clone(),
            line,
            column,
            code: said.code,
            message: said.message,
        });
    }
    if !elsewhere.is_empty() {
        return Err(TsError::Declarations {
            said: elsewhere.join("\n"),
        });
    }
    findings.sort_by(|a, b| (&a.key, a.line, a.column).cmp(&(&b.key, b.line, b.column)));
    findings.dedup();
    Ok(Outcome::Checked {
        files: modules.into_iter().map(|(_, key, _)| key).collect(),
        findings,
    })
}

/// Whether a script's key stays inside the project, so that it can be
/// written under the scratch directory.
fn within(key: &str) -> bool {
    !key.starts_with('/') && !key.contains(':') && key.split('/').all(|part| part != "..")
}

/// A directory of its own under the system's temporary one, gone when the
/// check is over. Its name is random and it is made new, so a directory
/// someone made beforehand is never written through.
struct Scratch(tempfile::TempDir);

impl Scratch {
    fn new() -> Result<Self, TsError> {
        tempfile::Builder::new()
            .prefix("ljos-tsc-")
            .tempdir()
            .map(Self)
            .map_err(|source| TsError::Io {
                path: std::env::temp_dir(),
                source,
            })
    }

    fn path(&self) -> &Path {
        self.0.path()
    }
}
