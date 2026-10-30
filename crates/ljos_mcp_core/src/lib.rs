//! MCP plumbing shared by every Ljós MCP server (ARCHITECTURE.md §19).
//!
//! Everything a server implements is expressed against [`McpSurface`] and the
//! SDK-neutral types in this crate. Only [`glue`] names `rmcp`, so swapping the
//! SDK or absorbing a breaking release touches one file. No egui, no engine.
//!
//! The feature `server` (on by default) brings `rmcp` and `tokio`, and [`glue`]
//! with them. Without it the crate is the protocol alone: requests and answers
//! ([`protocol`]), the tool descriptions and capabilities, and it builds for
//! `wasm32-unknown-unknown`.

#[cfg(feature = "http")]
mod alive;
#[cfg(feature = "http")]
mod auth;
mod caps;
mod error;
#[cfg(feature = "server")]
pub mod glue;
#[cfg(feature = "inspect")]
mod locator;
pub mod protocol;
#[cfg(feature = "inspect")]
mod refs;
mod registry;
#[cfg(feature = "remote")]
pub mod remote;
mod reply;
mod size;
#[cfg(feature = "inspect")]
mod snapshot;
mod socket;
mod surface;

#[cfg(feature = "http")]
pub use alive::{effective_uid, process_alive};
#[cfg(feature = "http")]
pub use auth::{
    RateLimit, Token, TokenFile, TokenFileError, address_host, loopback_host, loopback_origin,
};
pub use protocol::{
    CALLS_PER_SECOND, DEFAULT_CALLS_PER_SECOND, RateError, check_calls_per_second,
    parse_calls_per_second,
};

pub use caps::{Cap, CapSet, ParseCapError};
pub use error::{ErrorCode, SurfaceError};
#[cfg(feature = "inspect")]
pub use locator::{At, Locator, LocatorError, NameTest, Point, STATES, Step, StepFilter};
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
pub use socket::{SOCKET_PATH_MAX, SocketPathError, check_socket_path};
pub use surface::{
    McpSurface, PromptArg, PromptDef, PromptText, ResourceContent, ResourceDef,
    ResourceTemplateDef, ServerMeta,
};
