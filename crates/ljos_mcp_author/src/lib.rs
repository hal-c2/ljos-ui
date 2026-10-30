//! The Ljós authoring MCP server (`ljos mcp`, ARCHITECTURE.md §19.3).
//!
//! Read-only analysis for coding agents that write QML. Everything is confined
//! to the `--project` directory. The QML front end is behind [`FrontEnd`]:
//! [`QmlFrontEnd`] is the real one (`qml_check`, `qml_lint`, `type_lookup`),
//! and [`StubFrontEnd`] stands in where no parser or Qt type table is wanted;
//! its tools answer with an `unavailable` tool error.

pub mod api;
mod blocks;
mod design;
mod docs;
mod edit;
mod features;
mod format;
mod front;
mod frontend;
mod ledger;
mod lints;
mod project;
mod prompts;
mod resources;
mod rewrite;
mod runs;
mod server;
#[cfg(test)]
mod skills;
mod tiers;
mod tools;

pub use front::{ApiUse, QmlFrontEnd, Resolution, Usage};
pub use frontend::{Diagnostic, FrontEnd, ModuleInfo, Severity, StubFrontEnd, TypeInfo};
pub use ledger::{ApiStatus, Ledger, LedgerError, LedgerRow, MemberKind};
pub use lints::{DECLARATIVE, IMPLEMENTED, Lint, lint, lints};
pub use project::{Project, ProjectError};
pub use server::{AuthorServer, serve_stdio};
