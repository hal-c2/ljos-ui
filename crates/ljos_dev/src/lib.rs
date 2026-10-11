//! Dev tools for Ljós: hot reload (ARCHITECTURE.md §12.1).
//!
//! ```text
//! file event → 120 ms quiet → fingerprints → compile → diff → ReloadPatch
//!   → the application's inbox → applied between two frames, whole or not
//!   at all → on any error the old generation stays, the overlay says why,
//!   and the watcher goes on watching
//! ```
//!
//! - [`debounce`]: the quiet time, on any clock.
//! - [`fingerprint`]: which saves changed nothing.
//! - [`reloader`]: saves in, a patch out; no thread and no clock.
//! - [`watch`]: the thread that sleeps on file events and runs the above.
//! - [`host`]: the application's side, and [`DevSession`] which is all of
//!   it for a host with one root.
//! - [`overlay`]: what the error overlay says.
//! - [`text`]: QML a script holds as text (`Qt.createQmlObject`).
//!
//! This crate compiles QML, so it links the parser. A shipped program
//! does not depend on it (CLAUDE.md rule 5): `ljos_shell` takes it behind
//! its `dev` feature only.
//!
//! Also here, and apart from hot reload:
//!
//! - [`top`]: the model of the `top` overlay, "which component ate the
//!   frame" (§9.1, §3.4).
//! - `mcp` (feature `mcp`, off by default): the read-only MCP inspector
//!   of a running application (§19.4 to §19.7).
//!
//! The devtools socket for a device comes with a later milestone.

pub mod debounce;
pub mod fingerprint;
pub mod host;
#[cfg(feature = "mcp")]
pub mod mcp;
pub mod overlay;
pub mod reloader;
pub mod text;
pub mod top;
pub mod watch;

pub use debounce::{Debounce, QUIET};
pub use fingerprint::Fingerprints;
pub use host::{
    DevError, DevSession, Host, ReloadStats, Report, STATE_PROPERTIES, apply, component_source,
    fault_of, prepare,
};
pub use overlay::Overlay;
pub use reloader::{Build, Reloader, Touch, classify};
pub use text::{ProjectTexts, text_compiler};
pub use top::{FrameStats, Top, TopRow, TopView};
pub use watch::{DevMsg, WatchError, Watcher};
