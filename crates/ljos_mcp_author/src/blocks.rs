//! `blocks_list`: the functional blocks of the project (ADR 0003), read by
//! `ljos_blocks` from its qmldir files and type dumps without running it.

use std::sync::Arc;

use ljos_blocks::BlockRegistry;
use ljos_mcp_core::{CallCtx, ErrorCode, SurfaceError, ToolHandler, typed};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Project, tools};

const PAGE: usize = 50;

pub(crate) const DESCRIPTION: &str = "List the project's functional blocks (`# block` qmldir comments, `#[qml(block)]` types in ljostypes.json) with their contracts: state, inputs, actions, events and slots. Declarations that could not be read are in `errors`. Paginated.";

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListArgs {
    /// Only the blocks of this module, for example `App.Media`.
    module: Option<String>,
    /// `next_cursor` of the previous answer.
    cursor: Option<String>,
}

#[derive(Serialize, JsonSchema)]
pub(crate) struct ListOut {
    /// The registry's format, as `ljos_blocks` writes it: 1.
    format: u32,
    /// Ordered by module, then name. Each has `module`, `name`, `version`,
    /// `origin` (`{"kind": "rust"}` or `{"kind": "qml", "file": ...}`),
    /// optional `category` and `summary`, and the lists `state`, `inputs`
    /// (`name`, `type`, `notify`), `actions` (`name`, `params`, `returns`),
    /// `events` (`name`, `params`) and `slots` (`name`, `accepts`).
    blocks: Vec<serde_json::Value>,
    /// Declarations that could not be read (`file`, `line`, `message`):
    /// a malformed `# block` comment, a missing file, a file that does not
    /// parse, a block declared twice. On the first page only, as many as
    /// fit a quarter of the answer's budget.
    errors: Vec<serde_json::Value>,
    /// How many declarations could not be read, listed or not.
    errors_total: usize,
    next_cursor: Option<String>,
}

pub(crate) fn handler(project: &Arc<Project>) -> ToolHandler {
    let project = project.clone();
    typed(move |_: &CallCtx, args: ListArgs| {
        let registry = BlockRegistry::from_project(project.root())
            .map_err(|e| SurfaceError::new(ErrorCode::Internal, e.to_string()))?;
        let registry = match &args.module {
            Some(module) => registry.of_module(module),
            None => registry,
        };
        let value = registry.to_value();
        let list = |key: &str| value[key].as_array().cloned().unwrap_or_default();
        let (blocks, next_cursor) =
            tools::page(list("blocks"), args.cursor.as_deref(), PAGE, tools::BUDGET)?;
        let errors = list("errors");
        let errors_total = errors.len();
        // Every page would repeat them; the first is where an agent looks.
        let errors = if args.cursor.is_none() {
            tools::page(errors, None, PAGE, tools::BUDGET / 4)?.0
        } else {
            Vec::new()
        };
        Ok(ListOut {
            format: BlockRegistry::FORMAT,
            blocks,
            errors,
            errors_total,
            next_cursor,
        })
    })
}
