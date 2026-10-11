//! `qml_format`: the layout of `qml_fmt`, which `ljos fmt` gives too.
//!
//! Without the `fs` capability the tool shows what it would change, as a
//! unified diff per file; with it and `write: true` it changes the files.
//! A file that does not parse is refused with what the parser said, and is
//! never written.

use std::{path::Path, sync::Arc};

use ljos_mcp_core::{CallCtx, Cap, ErrorCode, SurfaceError, ToolHandler, typed};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Project, Severity, tools};

/// Files in one answer, at most.
const PAGE: usize = 50;
/// A file's diff is cut here: a file that changes this much is read whole
/// more cheaply than as a diff.
const DIFF_BYTES: usize = 6 * 1024;
/// Once the diffs of an answer add up to this, the rest of the files wait
/// for the next call, so the answer stays under the response cap (§19.6).
const BUDGET: usize = 10 * 1024;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FormatArgs {
    /// `.qml` files or directories, relative to the project root.
    #[serde(default)]
    paths: Vec<String>,
    /// QML text to format instead of files. The result is in `formatted`.
    source: Option<String>,
    /// Rewrite the files instead of only showing the diff. Needs the `fs`
    /// capability.
    #[serde(default)]
    write: bool,
    /// `next_cursor` of the previous answer: formats the files it left.
    cursor: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum Status {
    /// Already laid out.
    Unchanged,
    /// Not laid out; `diff` shows what would change. Nothing was written.
    Changed,
    /// Was not laid out and has been rewritten; `diff` shows what changed.
    Written,
    /// Does not parse (see `diagnostics`), or could not be read or
    /// written. Left as it was.
    Refused,
}

#[derive(Serialize, JsonSchema)]
struct Problem {
    line: u32,
    column: u32,
    severity: Severity,
    message: String,
}

#[derive(Serialize, JsonSchema)]
struct FileOut {
    /// Project-relative path; `<source>` for text given as `source`.
    file: String,
    status: Status,
    /// Unified diff from the file to its layout, when the two differ.
    diff: Option<String>,
    /// The diff was longer than 6 KB and was cut at a line.
    diff_truncated: bool,
    /// Why a refused file was refused.
    diagnostics: Vec<Problem>,
}

#[derive(Serialize, JsonSchema)]
pub(crate) struct FormatOut {
    /// One entry per file handled by this call, in path order.
    files: Vec<FileOut>,
    /// Counts over `files`.
    unchanged: usize,
    changed: usize,
    written: usize,
    refused: usize,
    /// The laid-out text, when `source` was given and parses.
    formatted: Option<String>,
    /// Set when files are left: they were neither checked nor written.
    /// Call again with this as `cursor`.
    next_cursor: Option<String>,
}

/// What became of one text.
struct Laid {
    status: Status,
    formatted: Option<String>,
    diagnostics: Vec<Problem>,
}

fn problem(line: u32, column: u32, message: impl Into<String>) -> Problem {
    Problem {
        line,
        column,
        severity: Severity::Error,
        message: message.into(),
    }
}

fn lay_out(source: &str) -> Laid {
    match qml_fmt::format(source) {
        Ok(formatted) if formatted == source => Laid {
            status: Status::Unchanged,
            formatted: Some(formatted),
            diagnostics: Vec::new(),
        },
        Ok(formatted) => Laid {
            status: Status::Changed,
            formatted: Some(formatted),
            diagnostics: Vec::new(),
        },
        Err(qml_fmt::Error::Syntax(errors)) => Laid {
            status: Status::Refused,
            formatted: None,
            diagnostics: errors
                .into_iter()
                .map(|e| problem(e.line, e.column, e.message))
                .collect(),
        },
        Err(e @ qml_fmt::Error::Unstable(_)) => Laid {
            status: Status::Refused,
            formatted: None,
            diagnostics: vec![problem(1, 1, e.to_string())],
        },
    }
}

/// The diff, cut at a line once it is over [`DIFF_BYTES`].
fn diff(name: &str, old: &str, new: &str) -> (String, bool) {
    let full = qml_fmt::unified_diff(name, old, new);
    if full.len() <= DIFF_BYTES {
        return (full, false);
    }
    let mut cut = DIFF_BYTES;
    while !full.is_char_boundary(cut) {
        cut -= 1;
    }
    let cut = full[..cut].rfind('\n').map_or(cut, |at| at + 1);
    (full[..cut].to_owned(), true)
}

fn file(project: &Project, path: &Path, write: bool) -> FileOut {
    let name = project.display(path);
    let refused = |message: String| FileOut {
        file: name.clone(),
        status: Status::Refused,
        diff: None,
        diff_truncated: false,
        diagnostics: vec![problem(1, 1, message)],
    };
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(e) => return refused(format!("cannot be read: {e}")),
    };
    let laid = lay_out(&source);
    let mut out = FileOut {
        file: name.clone(),
        status: laid.status,
        diff: None,
        diff_truncated: false,
        diagnostics: laid.diagnostics,
    };
    if let (Status::Changed, Some(formatted)) = (laid.status, &laid.formatted) {
        if write {
            if let Err(e) = std::fs::write(path, formatted) {
                return refused(format!("cannot be written: {e}"));
            }
            out.status = Status::Written;
        }
        let (text, truncated) = diff(&name, &source, formatted);
        out.diff = Some(text);
        out.diff_truncated = truncated;
    }
    out
}

fn answer(
    files: Vec<FileOut>,
    formatted: Option<String>,
    next_cursor: Option<String>,
) -> FormatOut {
    let count = |status| files.iter().filter(|f| f.status == status).count();
    FormatOut {
        unchanged: count(Status::Unchanged),
        changed: count(Status::Changed),
        written: count(Status::Written),
        refused: count(Status::Refused),
        files,
        formatted,
        next_cursor,
    }
}

pub(crate) const DESCRIPTION: &str = "Lay out QML files, directories or `source` text the one way `ljos fmt` does. Answers a unified diff per file; `write: true` rewrites the files and needs the `fs` capability. A file that does not parse is refused with its diagnostics and left alone.";

pub(crate) fn handler(project: &Arc<Project>) -> ToolHandler {
    let project = project.clone();
    typed(move |ctx: &CallCtx, args: FormatArgs| {
        if args.write && !ctx.caps.contains(Cap::Fs) {
            return Err(SurfaceError::new(
                ErrorCode::CapabilityDenied,
                "`write: true` needs the `fs` capability, which this session was not started with \
                 (`ljos mcp --caps fs`); without `write` the answer is the diff",
            )
            .with_details(serde_json::json!({ "required": Cap::Fs, "granted": ctx.caps })));
        }
        if let Some(source) = &args.source {
            if !args.paths.is_empty() {
                return Err(SurfaceError::invalid_argument(
                    "give `paths` or `source`, not both",
                ));
            }
            if args.write {
                return Err(SurfaceError::invalid_argument(
                    "`write` rewrites files; with `source` the result is in `formatted`",
                ));
            }
            let laid = lay_out(source);
            let name = "<source>".to_owned();
            let (diff, diff_truncated) = match (&laid.formatted, laid.status) {
                (Some(formatted), Status::Changed) => {
                    let (text, truncated) = diff(&name, source, formatted);
                    (Some(text), truncated)
                }
                _ => (None, false),
            };
            let out = FileOut {
                file: name,
                status: laid.status,
                diff,
                diff_truncated,
                diagnostics: laid.diagnostics,
            };
            return Ok(answer(vec![out], laid.formatted, None));
        }
        if args.paths.is_empty() {
            return Err(SurfaceError::invalid_argument(
                "give `paths` (files or directories, for example [\".\"]) or `source` (QML text)",
            ));
        }
        let files = tools::expand_qml(&project, &args.paths)?;
        let start = tools::cursor(args.cursor.as_deref())?;
        let mut out = Vec::new();
        let mut spent = 0;
        let mut next = start.min(files.len());
        // A file is handled only if it will be in the answer: one that was
        // written and then dropped from the list would never be reported.
        for path in files.iter().skip(start) {
            if out.len() == PAGE || spent >= BUDGET {
                break;
            }
            let one = file(&project, path, args.write);
            spent += one.diff.as_ref().map_or(0, String::len)
                + one
                    .diagnostics
                    .iter()
                    .map(|d| d.message.len() + 64)
                    .sum::<usize>()
                + one.file.len()
                + 96;
            out.push(one);
            next += 1;
        }
        let next_cursor = (next < files.len()).then(|| next.to_string());
        Ok(answer(out, None, next_cursor))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_diff_is_cut_at_a_line() {
        let old: String = (0..2000).map(|i| format!("line {i}\n")).collect();
        let new: String = (0..2000).map(|i| format!("row {i}\n")).collect();
        let (text, truncated) = diff("A.qml", &old, &new);
        assert!(truncated);
        assert!(text.len() <= DIFF_BYTES);
        assert!(text.ends_with('\n'));
        let (text, truncated) = diff("A.qml", "a\n", "b\n");
        assert!(!truncated);
        assert!(text.contains("+b"));
    }
}
