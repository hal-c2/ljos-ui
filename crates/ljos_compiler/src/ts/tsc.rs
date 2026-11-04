//! TypeScript itself, run on modules [`super::extract`] wrote: `deno check`,
//! or the `tsc` the variable `LJOS_TSC` names. Both are held to the same
//! strict settings, written beside the modules for each run.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use super::TsError;

/// The settings TypeScript checks with: the strict ones of
/// docs/DIALECT.md §4, and no library but the language's own.
pub const COMPILER_OPTIONS: &str = r#"{
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true,
    "noImplicitAny": true,
    "lib": ["es2022"]
  }"#;

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

    /// What TypeScript says of `files`, modules in `dir`.
    pub fn check(&self, dir: &Path, files: &[String]) -> Result<Vec<Said>, TsError> {
        let (mut command, config) = match self {
            Checker::Deno(deno) => {
                let mut command = Command::new(deno);
                command.args(["check", "--quiet", "--no-lock", "--config", "deno.json"]);
                (
                    command,
                    (
                        "deno.json",
                        format!("{{\n  \"compilerOptions\": {COMPILER_OPTIONS}\n}}\n"),
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
                let options = COMPILER_OPTIONS.replacen('{', "{\n    \"noEmit\": true,", 1);
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
        let said = match self {
            Checker::Deno(_) => parse_deno(&text),
            Checker::Tsc(_) => parse_tsc(&text),
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
    /// The module, by its name in the directory.
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
fn parse_deno(text: &str) -> Vec<Said> {
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
        let Some((file, line, column)) = location(at) else {
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

/// `file:///dir/a.ts:4:30`: the file's name, the line and the column.
fn location(at: &str) -> Option<(String, u32, u32)> {
    let (rest, column) = at.trim().rsplit_once(':')?;
    let (path, line) = rest.rsplit_once(':')?;
    let name = path.rsplit('/').next()?.to_owned();
    Some((name, line.parse().ok()?, column.parse().ok()?))
}

/// `a.ts(4,30): error TS2322: message`.
fn parse_tsc(text: &str) -> Vec<Said> {
    text.lines()
        .filter_map(|line| {
            let (head, rest) = line.split_once("): error ")?;
            let (path, position) = head.rsplit_once('(')?;
            let (line, column) = position.split_once(',')?;
            let (code, message) = rest.split_once(": ")?;
            Some(Said {
                file: path.rsplit(['/', '\\']).next()?.to_owned(),
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
        assert_eq!(parse_deno(deno), want);
        assert_eq!(parse_tsc(tsc), want);
    }
}
