//! A client of a running application's inspector (ARCHITECTURE.md §19.7),
//! for any program in another process that drives one: the token file that
//! names it (`token`), one JSON-RPC message posted to it as streamable
//! HTTP on the loopback address or its 0600 Unix socket (`wire`), and an
//! MCP client on top ([`Client`], [`BlockingClient`]).
//!
//! Every call is one `POST`, answered on its own as the inspector's
//! stateless mode answers it, so a client holds no connection and no
//! timer: between calls it costs nothing, and an application started again
//! is reached again by reading its token file anew.

mod client;
mod token;
mod wire;

pub use client::{BlockingClient, Client, Image, PATIENCE, ToolError, ToolResult};
pub use token::{
    MAX_TOKEN_FILE, Target, TokenError, Trust, find_token_file, read_token_file, token_file_of,
    wait_for_token, wait_for_token_blocking,
};
pub use wire::{Answer, IN_FLIGHT, MAX_BODY_BYTES, MAX_RETRY_AFTER, Unreached, post, spawn};
