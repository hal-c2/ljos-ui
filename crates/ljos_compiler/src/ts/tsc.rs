//! TypeScript itself, run on modules [`super::extract`] wrote: `deno check`,
//! or the `tsc` the variable `LJOS_TSC` names. Both are held to the same
//! strict settings, written beside the modules for each run.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use super::TsError;

/// The settings TypeScript checks with: the strict ones of
/// docs/DIALECT.md §4, and no library but the language's own, with
/// `using`'s disposables (§4.22).
pub const COMPILER_OPTIONS: &str = r#"{
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true,
    "noImplicitAny": true,
    "allowImportingTsExtensions": true,
    "lib": ["es2022", "esnext.disposable"]
  }"#;

/// Where the modules TypeScript reads are, beside the files it checks.
/// An `.lts` module is not there as itself: an import of it, from a QML
/// file, a module or a body, is read as its declaration, as stock
/// TypeScript reads `.ljos/types/` through `rootDirs`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layout {
    /// The `.ts` modules, by their keys under [`super::MODULES`].
    pub modules: Vec<String>,
    /// The `.lts` modules, by their keys, and the paths of their
    /// declarations.
    pub lts: Vec<(String, String)>,
}

impl Layout {
    /// The options that let TypeScript find the modules: `rootDirs` makes
    /// the modules, declarations and bodies one directory, and
    /// `allowArbitraryExtensions` reads `width.d.lts.ts` for `./width.lts`.
    fn tsc_options(&self) -> &'static str {
        if self.lts.is_empty() {
            ""
        } else {
            concat!(
                "\n    \"allowArbitraryExtensions\": true,",
                "\n    \"rootDirs\": [\"m\", \"types\", \"check\"],"
            )
        }
    }

    /// Deno has no `rootDirs`: an import map sends what a body imports, and
    /// every import of an `.lts` module, where the module would be.
    fn deno_imports(&self) -> String {
        let mut map: Vec<(String, String)> = Vec::new();
        // What `rootDirs` makes one tree for tsc: an import from any of the
        // three directories finds a module in `m`, and an `.lts` module's
        // declaration in `types`. A declaration imports what its module
        // imports, so `types` is one of the three.
        for (key, declaration) in &self.lts {
            for dir in [super::MODULES, super::BODIES, crate::lts::TYPES] {
                map.push((format!("./{dir}/{key}"), format!("./{declaration}")));
            }
        }
        if !self.lts.is_empty() {
            for key in &self.modules {
                for dir in [super::BODIES, crate::lts::TYPES] {
                    map.push((
                        format!("./{dir}/{key}"),
                        format!("./{}/{key}", super::MODULES),
                    ));
                }
            }
        }
        if map.is_empty() {
            return String::new();
        }
        let entries = map
            .iter()
            .map(|(from, to)| format!("    {from:?}: {to:?}"))
            .collect::<Vec<_>>()
            .join(",\n");
        format!(",\n  \"imports\": {{\n{entries}\n  }}")
    }
}

/// A program that checks TypeScript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checker {
    /// `deno check`, with the TypeScript it comes with.
    Deno(PathBuf),
    /// `tsc --noEmit`.
    Tsc(PathBuf),
}

impl Checker {
    /// `LJOS_TSC` when it is set, else `deno` on the `PATH`; None when there
    /// is neither.
    pub fn find() -> Option<Self> {
        if let Some(tsc) = std::env::var_os("LJOS_TSC").filter(|v| !v.is_empty()) {
            return Some(Self::Tsc(PathBuf::from(tsc)));
        }
        on_path("deno").map(Self::Deno)
    }

    /// What TypeScript says of `files`, modules in `dir` laid out as
    /// `layout` says.
    pub fn check(
        &self,
        dir: &Path,
        files: &[String],
        layout: &Layout,
    ) -> Result<Vec<Said>, TsError> {
        let (mut command, config) = match self {
            Checker::Deno(deno) => {
                let mut command = Command::new(deno);
                command.args(["check", "--quiet", "--no-lock", "--config", "deno.json"]);
                (
                    command,
                    (
                        "deno.json",
                        format!(
                            "{{\n  \"compilerOptions\": {COMPILER_OPTIONS}{}\n}}\n",
                            layout.deno_imports()
                        ),
                    ),
                )
            }
            Checker::Tsc(tsc) => {
                let mut command = Command::new(tsc);
                command.args(["--noEmit", "--pretty", "false", "-p", "tsconfig.json"]);
                let list = files
                    .iter()
                    .map(|f| format!("{f:?}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let options = COMPILER_OPTIONS.replacen(
                    '{',
                    &format!("{{\n    \"noEmit\": true,{}", layout.tsc_options()),
                    1,
                );
                (
                    command,
                    (
                        "tsconfig.json",
                        format!(
                            "{{\n  \"compilerOptions\": {options},\n  \"files\": [{list}]\n}}\n"
                        ),
                    ),
                )
            }
        };
        let path = dir.join(config.0);
        std::fs::write(&path, config.1).map_err(|source| TsError::Io { path, source })?;
        if matches!(self, Checker::Deno(_)) {
            command.args(files);
        }
        let out = command
            .current_dir(dir)
            .env("NO_COLOR", "1")
            .env("DENO_NO_UPDATE_CHECK", "1")
            .stdin(Stdio::null())
            .output()
            .map_err(|source| TsError::Start {
                program: self.program().to_owned(),
                source,
            })?;
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        // TypeScript may give a file by the path it resolved the directory to.
        let real = dir.canonicalize().unwrap_or_else(|_| dir.to_owned());
        let said = match self {
            Checker::Deno(_) => parse_deno(&text, &real),
            Checker::Tsc(_) => parse_tsc(&text, &real),
        };
        if said.is_empty() && !out.status.success() {
            return Err(TsError::Failed {
                program: self.program().to_owned(),
                output: text.trim().to_owned(),
            });
        }
        Ok(said)
    }

    fn program(&self) -> &Path {
        match self {
            Checker::Deno(path) | Checker::Tsc(path) => path,
        }
    }
}

fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// One error TypeScript reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    /// The module, by its path in the directory, with `/` between its
    /// parts: `f0.ts`, `m/filter.ts`.
    pub file: String,
    pub line: u32,
    pub column: u32,
    /// `TS2322`.
    pub code: String,
    /// The first line of what it says: the rest elaborates on it.
    pub message: String,
}

/// `TS2322 [ERROR]: message`, its source lines, then
/// `    at file:///dir/a.ts:4:30`.
fn parse_deno(text: &str, dir: &Path) -> Vec<Said> {
    let mut said = Vec::new();
    let mut pending: Option<(String, String)> = None;
    for line in text.lines() {
        if let Some((code, rest)) = line.split_once(" [ERROR]: ")
            && code.starts_with("TS")
            && code[2..].chars().all(|c| c.is_ascii_digit())
        {
            pending = Some((code.to_owned(), rest.trim().to_owned()));
            continue;
        }
        let Some(at) = line.trim_start().strip_prefix("at ") else {
            continue;
        };
        let Some((code, message)) = pending.take() else {
            continue;
        };
        let Some((file, line, column)) = location(at, dir) else {
            continue;
        };
        said.push(Said {
            file,
            line,
            column,
            code,
            message,
        });
    }
    said
}

/// `file:///dir/a.ts:4:30`: the file's path in `dir`, the line and the
/// column.
fn location(at: &str, dir: &Path) -> Option<(String, u32, u32)> {
    let (rest, column) = at.trim().rsplit_once(':')?;
    let (path, line) = rest.rsplit_once(':')?;
    let path = path.strip_prefix("file://").unwrap_or(path);
    Some((in_dir(path, dir)?, line.parse().ok()?, column.parse().ok()?))
}

/// The path `path` has in `dir`, which TypeScript may give whole or as it
/// is from `dir`; its name alone when it is neither.
fn in_dir(path: &str, dir: &Path) -> Option<String> {
    let path = path.replace('\\', "/");
    let dir = dir.to_string_lossy().replace('\\', "/");
    if let Some(rest) = path
        .strip_prefix(dir.trim_end_matches('/'))
        .and_then(|r| r.strip_prefix('/'))
    {
        return Some(rest.to_owned());
    }
    if !path.starts_with('/') && !path.contains(':') {
        return Some(path.trim_start_matches("./").to_owned());
    }
    path.rsplit('/').next().map(str::to_owned)
}

/// `a.ts(4,30): error TS2322: message`.
fn parse_tsc(text: &str, dir: &Path) -> Vec<Said> {
    text.lines()
        .filter_map(|line| {
            let (head, rest) = line.split_once("): error ")?;
            let (path, position) = head.rsplit_once('(')?;
            let (line, column) = position.split_once(',')?;
            let (code, message) = rest.split_once(": ")?;
            Some(Said {
                file: in_dir(path, dir)?,
                line: line.parse().ok()?,
                column: column.parse().ok()?,
                code: code.to_owned(),
                message: message.trim().to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deno_and_tsc_are_read_alike() {
        let deno = "TS2322 [ERROR]: Type 'string' is not assignable to type 'number'.\n    level = \"0\"\n    ~~~~~\n    at file:///x/f1.ts:12:5\n\nerror: Type checking failed.\n";
        let tsc = "f1.ts(12,5): error TS2322: Type 'string' is not assignable to type 'number'.\n";
        let want = vec![Said {
            file: "f1.ts".into(),
            line: 12,
            column: 5,
            code: "TS2322".into(),
            message: "Type 'string' is not assignable to type 'number'.".into(),
        }];
        assert_eq!(parse_deno(deno, Path::new("/x")), want);
        assert_eq!(parse_tsc(tsc, Path::new("/x")), want);
    }
}
