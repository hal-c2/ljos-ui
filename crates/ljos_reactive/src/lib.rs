//! `ljos_reactive`: the dependency graph under every QML binding
//! (ARCHITECTURE.md §6.2).
//!
//! # Model
//!
//! A [`Graph<V>`] holds two kinds of node, addressed by generational
//! [`NodeId`]s in a slotmap arena:
//!
//! - **Properties.** A value plus a stack of layers: a base layer (plain
//!   value or binding) and zero or more overrides (`Binding {}`,
//!   `PropertyChanges`). The top layer is active. Writing imperatively
//!   breaks the active binding, [`Graph::bind`] installs one
//!   (`Qt.binding()`), and [`Graph::remove_override`] restores the layer
//!   below according to a [`RestoreMode`]. Each property reports its
//!   [`Provenance`] for devtools and hot reload.
//! - **Effects.** Eager roots: signal handlers, Rust subscribers
//!   ([`Graph::watch`]), polish effects. They run synchronously, before the
//!   write that triggered them returns.
//!
//! Nodes belong to a [`ScopeId`]. Scopes form a tree mirroring the engine's
//! ownership boundaries; disposing a scope disposes its subtree
//! deterministically, and freezing a scope parks its effects.
//!
//! # Algorithm
//!
//! The graph is push-pull with three colours, in the style of Reactively:
//!
//! 1. **Push (mark).** A write that changes a value (by the graph's
//!    pluggable equality) or an explicit [`Graph::notify`] marks direct
//!    observers [`NodeState::Dirty`] and everything further downstream
//!    [`NodeState::Check`]. Marking stops at nodes that are already
//!    non-clean, so it touches each node at most once per change. Eager
//!    nodes reached by marking are queued by `(height, creation order)`.
//! 2. **Pull (update).** Reading a non-clean node brings it up to date. A
//!    `Check` node walks its sources in order, updating each; a source that
//!    recomputes to a *different* value marks its observers `Dirty`, and
//!    only then does the reader recompute. A source that recomputes to an
//!    equal value stops the wave (equality cut-off). The walk uses an
//!    explicit stack, so deep chains do not deepen the call stack.
//! 3. **Flush.** After a write (or at the end of the outermost
//!    [`Graph::batch`]) the queue drains in height order. Each root is
//!    pulled as in step 2, so it runs at most once per flush and only sees
//!    consistent values (glitch-free within a flush). Consecutive unbatched
//!    writes still flush separately, matching Qt's intermediate
//!    `onXChanged` calls.
//!
//! Edges are stored on both ends with the index of their mirror entry, and
//! child scopes form an intrusive list, so disposing a node or scope costs
//! the same in a graph of ten nodes or a million ([`Graph::check_invariants`]
//! verifies the indices). [`Graph::refresh`] re-runs a binding in place,
//! keeping its provenance and override stack; [`Graph::set_node_data`] tags a
//! node so diagnostics can be mapped back to a property name.
//!
//! Every evaluation records the nodes it reads (deduplicated with a stamp,
//! no allocation in steady state) and diffs them against the previous
//! sources, so dependencies follow the branches actually taken.
//!
//! **Lazy versus eager.** Bindings are lazy: they stay dirty until read, so
//! off-screen bindings cost nothing. Effects are eager. A binding that
//! writes to another property while it evaluates is promoted to an eager
//! root, because its side effect must not depend on who reads it.
//! [`Options::eager`] makes every binding eager, the `--eager` debug mode
//! for differential testing.
//!
//! **Loops.** Reading a node that is [`NodeState::Computing`] is a binding
//! loop: the read returns the stale value, the node is not re-entered, and a
//! [`Diagnostic::BindingLoop`] with the cycle is reported. Effects that keep
//! re-dirtying each other are capped per flush
//! ([`Diagnostic::RerunLimit`]).
//!
//! **Frozen scopes.** An effect owned by a frozen scope is parked when its
//! turn comes. It stays dirty, so further changes coalesce; on thaw it runs
//! once against the latest values. Bindings in a frozen scope are unaffected
//! (they are lazy already).
//!
//! # Threading and re-entrancy
//!
//! The graph is `!Send` and `!Sync` and takes no locks. All state sits in
//! one `RefCell` that is never borrowed while user code runs, so bindings
//! and effects receive `&Graph` and may read, write, create and dispose
//! nodes. A panicking binding leaves the graph consistent and retries on its
//! next read.
//!
//! # Example
//!
//! ```
//! use ljos_reactive::Graph;
//! use std::{cell::RefCell, rc::Rc};
//!
//! let g: Graph<i64> = Graph::new();
//! let root = g.root();
//! let width = g.property(root, 10).unwrap();
//! let area = g.binding(root, 0, move |g| g.get(width) * g.get(width)).unwrap();
//!
//! let seen = Rc::new(RefCell::new(Vec::new()));
//! let log = seen.clone();
//! g.watch(root, area, move |_, v| log.borrow_mut().push(*v)).unwrap();
//!
//! g.set(width, 3);
//! assert_eq!(*seen.borrow(), vec![9]);
//! ```

mod graph;
mod types;

pub use graph::{Graph, ScopeHandle};
pub use types::{
    BindingFn, Diagnostic, EffectFn, NodeId, NodeKind, NodeState, Options, OverrideId,
    OverrideValue, Provenance, ReactiveError, RestoreMode, ScopeId, Stats, WriteKind,
};
