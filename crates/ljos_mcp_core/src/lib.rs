//! MCP plumbing shared by every Ljós MCP server (ARCHITECTURE.md §19).
//!
//! Everything a server implements is expressed against [`McpSurface`] and the
//! SDK-neutral types in this crate. Only [`glue`] names `rmcp`, so swapping the
//! SDK or absorbing a breaking release touches one file. No egui, no engine.

mod caps;
mod error;
pub mod glue;
mod registry;
mod size;
mod surface;

pub use caps::{Cap, CapSet, ParseCapError};
pub use error::{ErrorCode, SurfaceError};
pub use registry::{
    CallCtx, NoArgs, ToolAnnotations, ToolDef, ToolHandler, ToolOutput, ToolRegistry,
    ToolRegistryError, typed,
};
pub use size::{DEFAULT_RESULT_BYTES, HARD_RESULT_BYTES, cap_result, paginate};
pub use surface::{
    McpSurface, PromptArg, PromptDef, PromptText, ResourceContent, ResourceDef,
    ResourceTemplateDef, ServerMeta,
};
