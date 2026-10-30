//! `qml_edit_apply`: one structured edit of a QML file through `qml_edit`,
//! which splices the author's text and leaves every byte outside the edited
//! member as it was.
//!
//! The answer is the splices, so a client that holds the text (an editor,
//! a designer) applies them itself. Without `write` it is a dry run that
//! also hands back the whole text when that fits; with `write` the file is
//! changed, which needs the `fs` capability as `qml_format` does.

use std::sync::Arc;

use ljos_mcp_core::{CallCtx, Cap, ErrorCode, SurfaceError, ToolHandler, typed};
use qml_edit::{Edit, NodePath};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Project, tools};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditArgs {
    /// The `.qml` file, relative to the project root.
    path: String,
    /// What to change, by `op`. Paths count objects as the file is before
    /// the edit.
    edit: EditSpec,
    /// Change the file instead of only answering the changes. Needs the
    /// `fs` capability.
    #[serde(default)]
    write: bool,
}

/// An object of the file: by its `id`, or by its path from the root object
/// (`root`, `root/0/2`, `root/contentItem:`, `root/component Name`), as
/// counted when the edit is made.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Address {
    id: Option<String>,
    path: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum EditSpec {
    /// Give `name` (dotted for a grouped property: `font.bold`) the QML
    /// `value`, written as after the `:`.
    SetBinding {
        object: Address,
        name: String,
        value: String,
    },
    RemoveBinding {
        object: Address,
        name: String,
    },
    /// Put the object written in `text` in `parent` as child `index` (the
    /// number of children: after the last).
    InsertObject {
        parent: Address,
        index: usize,
        text: String,
    },
    RemoveObject {
        object: Address,
    },
    /// Make `object` child `index` of `parent`, counted without it.
    MoveObject {
        object: Address,
        parent: Address,
        index: usize,
    },
}

#[derive(Serialize, JsonSchema)]
struct ChangeOut {
    /// Byte offsets into the file as it was.
    start: usize,
    end: usize,
    replacement: String,
}

#[derive(Serialize, JsonSchema)]
pub(crate) struct EditOut {
    /// Splices of the file as it was, in order, not overlapping.
    changes: Vec<ChangeOut>,
    /// The whole edited text, on a dry run when it fits in the answer.
    text: Option<String>,
    /// A dry run whose text did not fit: apply `changes` instead.
    text_omitted: bool,
    written: bool,
}

pub(crate) const DESCRIPTION: &str = "Edit one QML file structurally: set_binding, remove_binding, insert_object, move_object or remove_object, by `id` or path (`root/0`). Bytes outside the edited member stay as written. Answers the splices; `write: true` changes the file and needs `fs`.";

fn resolve(source: &str, address: Address) -> Result<NodePath, SurfaceError> {
    match (address.id, address.path) {
        (Some(id), None) => qml_edit::find_id(source, &id)
            .map_err(refusal)?
            .ok_or_else(|| SurfaceError::not_found(format!("no object has the id `{id}`"))),
        (None, Some(path)) => path
            .parse()
            .map_err(|e: qml_edit::BadPath| SurfaceError::invalid_argument(e.to_string())),
        _ => Err(SurfaceError::invalid_argument(
            "an object is named by `id` or by `path`, one of the two",
        )),
    }
}

fn plan(source: &str, spec: EditSpec) -> Result<Edit, SurfaceError> {
    Ok(match spec {
        EditSpec::SetBinding {
            object,
            name,
            value,
        } => Edit::SetBinding {
            object: resolve(source, object)?,
            name,
            value,
        },
        EditSpec::RemoveBinding { object, name } => Edit::RemoveBinding {
            object: resolve(source, object)?,
            name,
        },
        EditSpec::InsertObject {
            parent,
            index,
            text,
        } => Edit::InsertObject {
            parent: resolve(source, parent)?,
            index,
            text,
        },
        EditSpec::RemoveObject { object } => Edit::RemoveObject {
            object: resolve(source, object)?,
        },
        EditSpec::MoveObject {
            object,
            parent,
            index,
        } => Edit::MoveObject {
            object: resolve(source, object)?,
            parent: resolve(source, parent)?,
            index,
        },
    })
}

fn refusal(e: qml_edit::Error) -> SurfaceError {
    use qml_edit::Error as E;
    let code = match &e {
        E::Syntax(_) => ErrorCode::Syntax,
        E::NotFound(_) | E::NoBinding { .. } => ErrorCode::NotFound,
        E::NotAChild(_) | E::IntoItself => ErrorCode::InvalidArgument,
        E::Unsupported(_) => ErrorCode::Unsupported,
        E::WouldBreak(_) => ErrorCode::WouldBreak,
    };
    SurfaceError::new(code, e.to_string())
}

pub(crate) fn handler(project: &Arc<Project>) -> ToolHandler {
    let project = project.clone();
    typed(move |ctx: &CallCtx, args: EditArgs| {
        if args.write && !ctx.caps.contains(Cap::Fs) {
            return Err(SurfaceError::new(
                ErrorCode::CapabilityDenied,
                "`write: true` needs the `fs` capability, which this session was not started with \
                 (`ljos mcp --caps fs`); without `write` the answer is the changes",
            )
            .with_details(json!({ "required": Cap::Fs, "granted": ctx.caps })));
        }
        let path = project.confine(&args.path)?;
        if path.extension().is_none_or(|e| e != "qml") {
            return Err(SurfaceError::invalid_argument(format!(
                "`{}` is not a `.qml` file",
                args.path
            )));
        }
        let source = std::fs::read_to_string(&path)
            .map_err(|e| SurfaceError::not_found(format!("`{}` cannot be read: {e}", args.path)))?;
        let edit = plan(&source, args.edit)?;
        let applied = qml_edit::apply(&source, &edit).map_err(refusal)?;
        if args.write {
            std::fs::write(&path, &applied.text).map_err(|e| {
                SurfaceError::new(
                    ErrorCode::Internal,
                    format!("`{}` cannot be written: {e}", project.display(&path)),
                )
            })?;
        }
        let changes: Vec<ChangeOut> = applied
            .changes
            .into_iter()
            .map(|c| ChangeOut {
                start: c.range.start,
                end: c.range.end,
                replacement: c.replacement,
            })
            .collect();
        let spent: usize = changes.iter().map(|c| c.replacement.len() + 64).sum();
        let fits = spent + applied.text.len() <= tools::BUDGET;
        let dry = !args.write;
        Ok(EditOut {
            changes,
            text: (dry && fits).then_some(applied.text),
            text_omitted: dry && !fits,
            written: args.write,
        })
    })
}
