//! MCP plumbing shared by every Ljós MCP server (ARCHITECTURE.md §19).
//!
//! Everything a server implements is expressed against [`McpSurface`] and the
//! SDK-neutral types in this crate. Only [`glue`] names `rmcp`, so swapping the
//! SDK or absorbing a breaking release touches one file. No egui, no engine.

#[cfg(feature = "http")]
mod auth;
mod caps;
mod error;
pub mod glue;
#[cfg(feature = "inspect")]
mod locator;
#[cfg(feature = "inspect")]
mod refs;
mod registry;
mod reply;
mod size;
#[cfg(feature = "inspect")]
mod snapshot;
mod surface;

#[cfg(feature = "http")]
pub use auth::{RateLimit, Token, TokenFile, TokenFileError, loopback_host, loopback_origin};

pub use caps::{Cap, CapSet, ParseCapError};
pub use error::{ErrorCode, SurfaceError};
#[cfg(feature = "inspect")]
pub use locator::{Locator, LocatorError, NameTest, STATES, Step, StepFilter};
#[cfg(feature = "inspect")]
pub use refs::RefTable;
pub use registry::{
    CallCtx, NoArgs, ToolAnnotations, ToolDef, ToolHandler, ToolOutput, ToolRegistry,
    ToolRegistryError, denied, typed,
};
pub use reply::{ResourceFuture, ToolFuture, ToolImage, ToolReply, base64_decode, base64_encode};
pub use size::{DEFAULT_RESULT_BYTES, HARD_RESULT_BYTES, cap_result, paginate};
#[cfg(feature = "inspect")]
pub use snapshot::{
    Delta, LineOptions, MAX_NAME_CHARS, NodeValue, Outline, OutlineOptions, OutlineWriter,
    SnapNode, SnapTree, delta, number, render_line,
};
pub use surface::{
    McpSurface, PromptArg, PromptDef, PromptText, ResourceContent, ResourceDef,
    ResourceTemplateDef, ServerMeta,
};
