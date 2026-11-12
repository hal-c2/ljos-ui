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
pub use extract::{Back, Extracted, extract};
pub use tsc::{Checker, Layout};

/// The name the declarations are written under, beside the modules.
pub const DECLARATIONS: &str = "ljos.d.ts";

/// The directory, beside the modules, the project's TypeScript modules are
/// written under as they are laid out in the project, so that what one
/// imports from another is found.
pub const MODULES: &str = "m";

/// The directory, beside [`MODULES`], an `.lts` module's desugared body is
/// written under, as `<path>.lts.body.ts`, so TypeScript checks it too. Its
/// module's declaration is under [`crate::lts::TYPES`], as `ljos check`
/// writes it to `.ljos/types/`, and what the body imports is found in
/// [`MODULES`] and there (docs/DIALECT.md §4.21).
pub const BODIES: &str = "check";

/// What an `.lts` module's body is named after its key.
pub const BODY_SUFFIX: &str = ".body.ts";

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
    let scratch = Scratch::new()?;
    let mut scripts: Vec<String> = Vec::new();
    // Each module is TypeScript's desugared (docs/DIALECT.md §4.21), and
    // what it says is moved back to the module as written. An `.lts`
    // module is read twice: its importers read its declaration, as stock
    // TypeScript reads `.ljos/types/`, and its body is checked on its own.
    let mut backs: Vec<(String, Back)> = Vec::new();
    let mut layout = Layout::default();
    let mut bodies: Vec<String> = Vec::new();
    for (key, text) in project.scripts() {
        if !crate::script::module::is_module(key) || !within(key) {
            continue;
        }
        let mut desugared = qml_syntax::desugar(text);
        if let Some(back) = Back::new(text, &desugared.text, desugared.map) {
            backs.push((key.to_owned(), back));
            declare_sugar(&mut desugared.text);
        }
        if crate::script::module::is_lts(key) {
            let declaration = crate::lts::declaration_key(key);
            if let Some(declared) = crate::lts::declare(key, text) {
                write_in(&scratch.path().join(&declaration), &declared)?;
            }
            let body = format!("{BODIES}/{key}{BODY_SUFFIX}");
            write_in(&scratch.path().join(&body), &desugared.text)?;
            layout.lts.push((key.to_owned(), declaration));
            bodies.push(body);
        } else {
            write_in(&scratch.path().join(MODULES).join(key), &desugared.text)?;
            layout.modules.push(key.to_owned());
        }
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
            write_in(&scratch.path().join(MODULES).join(key), &text)?;
            layout.modules.push(key.to_owned());
        }
        scripts.push(key.to_owned());
    }
    // A file sees the modules of Ljós's API it imports and no others, so
    // the files that import the same ones are given to TypeScript together,
    // with declarations of those alone.
    let mut sets: Vec<Set> = Vec::new();
    let mut modules = Vec::new();
    let mut keys: Vec<&str> = project.files().collect();
    keys.sort_unstable();
    for key in keys {
        let imported = project.source(key).map(dts::modules_of).unwrap_or_default();
        if !imported.contains(&dts::API) {
            continue;
        }
        let set = match sets.iter().position(|set| set.modules == imported) {
            Some(set) => set,
            None => {
                let api = Api::of_all(&imported).ok_or(TsError::NoApi(dts::API))?;
                let declarations = match sets.len() {
                    0 => DECLARATIONS.to_owned(),
                    n => format!("ljos{n}.d.ts"),
                };
                let path = scratch.path().join(&declarations);
                std::fs::write(&path, api.declarations())
                    .map_err(|source| TsError::Io { path, source })?;
                sets.push(Set {
                    modules: imported,
                    api,
                    declarations,
                });
                sets.len() - 1
            }
        };
        let Some(extracted) = extract(
            project,
            &sets[set].api,
            key,
            &format!("./{}", sets[set].declarations),
            &format!("./{MODULES}"),
        ) else {
            continue;
        };
        let name = format!("f{}.ts", modules.len());
        let path = scratch.path().join(&name);
        std::fs::write(&path, &extracted.text).map_err(|source| TsError::Io { path, source })?;
        modules.push((name, key.to_owned(), extracted, set));
    }
    if modules.is_empty() && bodies.is_empty() {
        return Ok(Outcome::Checked {
            files: Vec::new(),
            findings: Vec::new(),
        });
    }
    let mut findings: Vec<Finding> = Vec::new();
    let mut elsewhere: Vec<String> = Vec::new();
    // The bodies go with the first set, or alone when no QML file is in
    // Ljós's API: they need no declarations of it.
    let runs = sets.len().max(usize::from(!bodies.is_empty()));
    for set in 0..runs {
        let mut names: Vec<String> = modules
            .iter()
            .filter(|(_, _, _, s)| *s == set)
            .map(|(name, _, _, _)| name.clone())
            .collect();
        if set == 0 {
            names.extend(bodies.iter().cloned());
        }
        if names.is_empty() {
            continue;
        }
        for said in checker.check(scratch.path(), &names, &layout)? {
            // A module is its own text desugared, line for line, so what
            // TypeScript says of it is at its own lines.
            let module = said
                .file
                .strip_prefix(MODULES)
                .and_then(|rest| rest.strip_prefix('/'))
                .or_else(|| {
                    said.file
                        .strip_prefix(BODIES)
                        .and_then(|rest| rest.strip_prefix('/'))
                        .and_then(|rest| rest.strip_suffix(BODY_SUFFIX))
                })
                .filter(|key| scripts.iter().any(|s| s == key));
            if let Some(key) = module {
                let (line, column) = backs
                    .iter()
                    .find(|(k, _)| k == key)
                    .map_or((said.line, said.column), |(_, back)| {
                        back.position(said.line, said.column)
                    });
                findings.push(Finding {
                    key: key.to_owned(),
                    line,
                    column,
                    code: said.code,
                    message: said.message,
                });
                continue;
            }
            // A declaration is written from a body TypeScript checks too:
            // what is wrong with it is wrong with the body, and said there.
            if let Some((key, _)) = layout.lts.iter().find(|(_, d)| *d == said.file)
                && findings.iter().any(|f| f.key == *key)
            {
                continue;
            }
            let Some((_, key, extracted, _)) =
                modules.iter().find(|(name, _, _, _)| *name == said.file)
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
    }
    if !elsewhere.is_empty() {
        return Err(TsError::Declarations {
            said: elsewhere.join("\n"),
        });
    }
    findings.sort_by(|a, b| (&a.key, a.line, a.column).cmp(&(&b.key, b.line, b.column)));
    findings.dedup();
    Ok(Outcome::Checked {
        files: modules.into_iter().map(|(_, key, _, _)| key).collect(),
        findings,
    })
}

/// What the desugar's calls are declared as to TypeScript (docs/DIALECT.md
/// §4.18): the runtime defines them, so a file that calls them declares
/// them, after its last line, which leaves its lines where they are.
pub const SUGAR: &str = "declare function $is<T>(s: unknown, p: (v: unknown) => v is T): s is T;\n\
declare function $is<T>(s: unknown, p: T): s is T;\n\
declare function $nomatch(x: never): never;\n";

/// Declares [`SUGAR`] at the end of the desugared `text`.
pub fn declare_sugar(text: &mut String) {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(SUGAR);
}

/// Writes `text` to `path`, with the directories it is in.
fn write_in(path: &Path, text: &str) -> Result<(), TsError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| TsError::Io {
            path: parent.to_owned(),
            source,
        })?;
    }
    std::fs::write(path, text).map_err(|source| TsError::Io {
        path: path.to_owned(),
        source,
    })
}

/// The files that import the same modules of Ljós's API, and the
/// declarations they are given.
struct Set {
    modules: Vec<&'static str>,
    api: Api,
    declarations: String,
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
