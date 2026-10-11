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
//!
//! This crate compiles QML, so it links the parser. A shipped program
//! does not depend on it (CLAUDE.md rule 5): `ljos_shell` takes it behind
//! its `dev` feature only.
//!
//! The devtools socket for a device, the inspector and the `top` overlay
//! that ARCHITECTURE.md also puts in this crate come with later
//! milestones.

pub mod debounce;
pub mod fingerprint;
pub mod host;
pub mod overlay;
pub mod reloader;
pub mod watch;

pub use debounce::{Debounce, QUIET};
pub use fingerprint::Fingerprints;
pub use host::{
    DevError, DevSession, Host, ReloadStats, Report, STATE_PROPERTIES, apply, component_source,
    fault_of, prepare,
};
pub use overlay::Overlay;
pub use reloader::{Build, Reloader, Touch, classify};
pub use watch::{DevMsg, WatchError, Watcher};
