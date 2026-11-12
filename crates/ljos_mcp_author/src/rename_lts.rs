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
            SurfaceError::new(
                ErrorCode::Internal,
                format!(
                    "the project cannot be read for `{}`'s importers: {e}",
                    args.path
                ),
            )
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
            rename.apply().map_err(|failed| {
                let said: Vec<String> = failed
                    .iter()
                    .map(|(path, e)| format!("`{}`: {e}", project.display(path)))
                    .collect();
                SurfaceError::new(
                    ErrorCode::Internal,
                    format!(
                        "the rename was undone, as these could not be written: {}",
                        said.join("; ")
                    ),
                )
            })?;
        }
        Ok(RenameOut {
            from: rename.from.clone(),
            to: rename.to.clone(),
            importers: rename.importers.iter().map(|i| i.key.clone()).collect(),
            written: args.write,
        })
    })
}
