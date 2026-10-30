//! Module `Ljos.Blocks` (ADR 0003 §4): the places a shell leaves open,
//! what the rest of the application puts in them, and a loader for a
//! shell a user may have broken.
//!
//! ```qml
//! import Ljos.Blocks
//! Row { Slot { name: "toolbar"; Text { text: "No tools" } } }
//! Contribution { slot: "toolbar"; ToolButton { text: "Save" } }
//! ShellLoader { source: "UserShell.qml"; fallback: "Shell.qml" }
//! ```
//!
//! The types are native rather than written in QML. A `Slot` finds every
//! live `Contribution` of its name through a registry the engine keeps
//! ([`registry`]), which a QML file has no way to: it would have to walk
//! the tree or be told by each contribution. A `ShellLoader` reports where
//! a shell failed, which needs the compiler's error and the faults raised
//! while the shell is made; a `Loader` only logs them.

mod contribution;
mod registry;
mod shell_loader;
mod slot;

use std::rc::Rc;

use ljos_engine::Engine;

pub use contribution::{Contribution, contribution_mode};
pub use shell_loader::{ShellLoader, shell_status};
pub use slot::Slot;

/// Registers module `Ljos.Blocks` and what makes a `ShellLoader` a
/// boundary.
pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!(
        "Ljos.Blocks" 1.0 => [Contribution, Slot, ShellLoader]
    ));
    engine.register_boundary::<ShellLoader>(Rc::new(shell_loader::Behaviour));
}
