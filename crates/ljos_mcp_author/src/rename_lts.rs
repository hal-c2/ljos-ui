//! `module_rename_lts`: the rename `ts-lts-syntax` names. A `.ts` module
//! that uses Ljós's syntax (`match`, `is`, `|>`) becomes an `.lts` module,
//! and every file of the project that imports it names the new file, as
//! `ljos migrate` renames it (docs/DIALECT.md §5.1).
//!
//! Without `write` the answer is the rename; with it the files are
//! changed, which needs the `fs` capability as `qml_edit_apply` does.

use std::sync::Arc;

use ljos_compiler::rename::{LtsPlan, plan_lts};
use ljos_mcp_core::{CallCtx, Cap, ErrorCode, SurfaceError, ToolHandler, typed};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::Project;

pub(crate) const DESCRIPTION: &str = "Rename a `.ts` module that uses Ljós's syntax (`match`, `is`, `|>`) to `.lts`, with the imports that name it in the project's QML files, scripts, modules and `qmldir`s: the fix `ts-lts-syntax` names. Answers the rename; `write: true` makes it and needs `fs`.";

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RenameArgs {
    /// The `.ts` module, relative to the project root.
    path: String,
    /// Rename the files instead of only answering the rename. Needs the
    /// `fs` capability.
    #[serde(default)]
    write: bool,
}

#[derive(Serialize, JsonSchema)]
pub(crate) struct RenameOut {
    /// The module's path from the project root.
    from: String,
    /// Its path once renamed, from the project root.
    to: String,
    /// The files whose imports name the module, from the project root.
    importers: Vec<String>,
    /// Whether the files were renamed and rewritten, or the rename only
    /// answered.
    written: bool,
}

pub(crate) fn handler(project: &Arc<Project>) -> ToolHandler {
    let project = project.clone();
    typed(move |ctx: &CallCtx, args: RenameArgs| {
        if args.write && !ctx.caps.contains(Cap::Fs) {
            return Err(SurfaceError::new(
                ErrorCode::CapabilityDenied,
                "`write: true` needs the `fs` capability, which this session was not started with \
                 (`ljos mcp --caps fs`); without `write` the answer is the rename",
            )
            .with_details(json!({ "required": Cap::Fs, "granted": ctx.caps })));
        }
        let path = project.confine(&args.path)?;
        if path.extension().is_none_or(|e| e != "ts") || args.path.ends_with(".d.ts") {
            return Err(SurfaceError::invalid_argument(format!(
                "`{}` is not a `.ts` module",
                args.path
            )));
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| SurfaceError::not_found(format!("`{}` cannot be read: {e}", args.path)))?;
        let plan = plan_lts(project.root(), &path, &text).map_err(|e| {
            let at = e.path.strip_prefix(project.root()).unwrap_or(&e.path);
            SurfaceError::new(
                ErrorCode::Internal,
                format!(
                    "`{}` cannot be read, so whether it imports `{}` is not known, and nothing \
                     is renamed: {}",
                    at.display(),
                    args.path,
                    e.error
                ),
            )
            .with_details(json!({ "unreadable": at.display().to_string() }))
        })?;
        let rename = match plan {
            LtsPlan::Rename(rename) => rename,
            LtsPlan::NoSyntax => {
                return Err(SurfaceError::invalid_argument(format!(
                    "`{}` has no Ljós syntax (`match`, `is`, `|>`), so stock TypeScript tools \
                     read it as it is; it stays `.ts`",
                    args.path
                )));
            }
            LtsPlan::Outside => {
                return Err(SurfaceError::invalid_argument(format!(
                    "`{}` is not in the project",
                    args.path
                )));
            }
            LtsPlan::InTheWay { to } => {
                return Err(SurfaceError::new(
                    ErrorCode::WouldBreak,
                    format!("`{to}` is already there; keep one of the two files"),
                ));
            }
        };
        if args.write {
            rename
                .apply()
                .map_err(|failed| not_written(&failed, |path| project.display(path)))?;
        }
        Ok(RenameOut {
            from: rename.from.clone(),
            to: rename.to.clone(),
            importers: rename.importers.iter().map(|i| i.key.clone()).collect(),
            written: args.write,
        })
    })
}

/// What a rename that could not be written says: the first of `failed` is
/// what could not be written, the rest what could not be put back after it.
/// Only a rename put back whole is said to be undone; one that was not
/// names each file left as the rename made it, for the author to put back.
fn not_written(
    failed: &[(std::path::PathBuf, std::io::Error)],
    shown: impl Fn(&std::path::Path) -> String,
) -> SurfaceError {
    let Some(((first, error), left)) = failed.split_first() else {
        return SurfaceError::new(ErrorCode::Internal, "the rename could not be written");
    };
    let first = shown(first);
    let message = if left.is_empty() {
        format!("the rename was undone, as `{first}` could not be written: {error}")
    } else {
        let said: Vec<String> = left
            .iter()
            .map(|(path, e)| format!("`{}`: {e}", shown(path)))
            .collect();
        format!(
            "`{first}` could not be written: {error}; these could not be put back and are as the rename left them, so put them back by hand: {}",
            said.join("; ")
        )
    };
    let left: Vec<String> = left.iter().map(|(path, _)| shown(path)).collect();
    SurfaceError::new(ErrorCode::Internal, message)
        .with_details(json!({ "failed": first, "not_put_back": left }))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::not_written;

    fn shown(path: &Path) -> String {
        path.display().to_string()
    }

    /// A rename put back whole says so, with what could not be written.
    #[test]
    fn a_rename_put_back_says_it_was_undone() {
        let failed = vec![(PathBuf::from("B.qml"), std::io::Error::other("disk full"))];
        let said = not_written(&failed, shown).to_string();
        assert!(said.contains("the rename was undone"), "{said}");
        assert!(
            said.contains("`B.qml` could not be written: disk full"),
            "{said}"
        );
    }

    /// A rename that could not be put back does not say it was undone: it
    /// names what failed first, and each file left as the rename made it.
    #[test]
    fn a_rename_not_put_back_names_what_is_left_and_does_not_say_undone() {
        let failed = vec![
            (PathBuf::from("B.qml"), std::io::Error::other("disk full")),
            (PathBuf::from("A.qml"), std::io::Error::other("disk full")),
            (PathBuf::from("size.lts"), std::io::Error::other("busy")),
        ];
        let error = not_written(&failed, shown);
        let said = error.to_string();
        assert!(!said.contains("undone"), "{said}");
        assert!(
            said.contains("`B.qml` could not be written: disk full"),
            "{said}"
        );
        assert!(said.contains("`A.qml`"), "{said}");
        assert!(said.contains("`size.lts`"), "{said}");
        let details = error.details.clone().unwrap_or_default();
        assert_eq!(details["failed"], "B.qml", "{details}");
        assert_eq!(
            details["not_put_back"],
            serde_json::json!(["A.qml", "size.lts"]),
            "{details}"
        );
    }
}
