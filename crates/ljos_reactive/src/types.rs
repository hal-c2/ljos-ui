//! Public vocabulary of the graph: ids, states, write kinds, diagnostics.

use std::fmt;

use crate::Graph;

slotmap::new_key_type! {
    /// A node in the graph: a property (plain or bound) or an effect.
    ///
    /// Keys are generational, so a disposed node reads as dead instead of
    /// aliasing a newer node.
    pub struct NodeId;

    /// An owner of nodes (ARCHITECTURE.md §3.4: one per `ProcId`-style
    /// boundary). Scopes form a tree, can be frozen, and dispose everything
    /// they own.
    pub struct ScopeId;
}

/// One run of a binding or effect, as [`Graph::reader`] gives it: a read
/// it made can be handed to a node made later ([`Graph::adopt_readers`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Reader {
    pub node: NodeId,
    pub run: u64,
}

/// Identifies one entry on a property's override stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OverrideId(pub(crate) u64);

/// The colour of a node in the push-pull algorithm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeState {
    /// The value is current.
    Clean,
    /// Some transitive source changed; the direct sources must be checked
    /// before the value can be trusted.
    Check,
    /// A direct source changed (or the binding was replaced); the node must
    /// re-run.
    Dirty,
    /// The node is running right now. Reading it again is a binding loop.
    Computing,
}

impl NodeState {
    pub(crate) fn rank(self) -> u8 {
        match self {
            NodeState::Clean => 0,
            NodeState::Check => 1,
            NodeState::Dirty => 2,
            NodeState::Computing => 3,
        }
    }
}

/// What kind of node an id refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// A property: holds a value, optionally driven by a binding or an
    /// override.
    Property,
    /// An eager effect root (signal handler, subscriber, polish effect…).
    Effect,
}

/// Where the current value of a property came from, as shown by devtools
/// (`app_inspect`) and used by hot reload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Provenance {
    /// Never written, never bound.
    Default,
    /// Written imperatively or initialised with a literal.
    Assigned,
    /// Produced by a binding.
    Bound,
    /// Written by an animation or Behavior; the binding, if any, stays
    /// installed and wins on its next evaluation.
    Animated,
    /// Written by the toolkit itself (a window's place, an item's focus, a
    /// timer that stopped); as with [`Provenance::Animated`], a binding
    /// stays installed.
    Host,
}

/// How a write treats the property's binding and override stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WriteKind {
    /// A JS or Rust assignment. Breaks the binding on the top layer of the
    /// override stack (QML semantics).
    Imperative,
    /// A creation-time value. Always targets the base layer, even when
    /// overrides are active; the engine uses it to skip Behaviors.
    Initial,
    /// An animation step. Sets the current value without removing any
    /// binding.
    Animation,
    /// A value the toolkit gives on its own account: what the window
    /// system, the focus chain or a running timer says. Treated as
    /// [`WriteKind::Animation`]; [`Provenance::Host`] tells them apart.
    Host,
    /// A value from whatever arranges the property's owner: a layout or a
    /// positioner placing a child. A binding on the top layer stays and
    /// gives the property its value again when it next runs; without one
    /// this is [`WriteKind::Imperative`].
    Layout,
    /// Writes the value of one override entry (for example a
    /// `PropertyChanges` whose value changed).
    Override(OverrideId),
}

/// Which earlier layer comes back when an override is removed. Mirrors
/// `Binding.restoreMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum RestoreMode {
    /// Keep the current value; the layer below becomes that plain value.
    None,
    /// Restore the layer below only if it is a binding.
    Binding,
    /// Restore the layer below only if it is a plain value.
    Value,
    /// Always restore the layer below.
    #[default]
    BindingOrValue,
}

/// A binding expression: reads other nodes through the graph and returns the
/// property's new value.
pub type BindingFn<V> = Box<dyn FnMut(&Graph<V>) -> V>;

/// An effect body.
pub type EffectFn<V> = Box<dyn FnMut(&Graph<V>)>;

/// The content of an override entry.
pub enum OverrideValue<V> {
    /// A constant value (`PropertyChanges { x: 5 }`).
    Value(V),
    /// A binding (`Binding { value: a + b }`).
    Binding(BindingFn<V>),
}

impl<V> OverrideValue<V> {
    /// Wraps a closure as a binding override.
    pub fn binding(f: impl FnMut(&Graph<V>) -> V + 'static) -> Self {
        OverrideValue::Binding(Box::new(f))
    }
}

impl<V: fmt::Debug> fmt::Debug for OverrideValue<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OverrideValue::Value(v) => f.debug_tuple("Value").field(v).finish(),
            OverrideValue::Binding(_) => f.write_str("Binding(..)"),
        }
    }
}

/// Problems the graph detects and recovers from. They are reported, never
/// panicked on; the engine maps node ids to QML names and prints Qt's text
/// (`QML <Type>: Binding loop detected for property "<name>"`).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Diagnostic {
    /// A binding's new value reached the binding again through its
    /// observers. The other members of the cycle were evaluated once
    /// against that value and the binding was not run again.
    #[error("binding loop detected for {node:?} (cycle: {cycle:?})")]
    BindingLoop {
        /// The binding whose change came back to it. Of a cycle's members
        /// it is the one that was evaluated first.
        node: NodeId,
        /// One cycle through `node`: each entry reads the next, and the
        /// last reads `node`.
        cycle: Vec<NodeId>,
    },
    /// An effect or eager binding was re-dirtied too often in one flush
    /// (handler ping-pong). Further runs in this flush were dropped.
    #[error("{node:?} re-ran {runs} times in one flush; further runs dropped")]
    RerunLimit {
        /// The node that hit the cap.
        node: NodeId,
        /// The cap that was hit.
        runs: u32,
    },
}

impl Diagnostic {
    /// The node the report is about: the binding a loop came back to, or
    /// the node that hit the re-run cap. Pass it to
    /// [`Graph::node_data`](crate::Graph::node_data) to name the property.
    pub fn node(&self) -> NodeId {
        match self {
            Diagnostic::BindingLoop { node, .. } | Diagnostic::RerunLimit { node, .. } => *node,
        }
    }

    /// The members of a binding loop, each reading the next; empty for
    /// other reports.
    pub fn cycle(&self) -> &[NodeId] {
        match self {
            Diagnostic::BindingLoop { cycle, .. } => cycle,
            Diagnostic::RerunLimit { .. } => &[],
        }
    }
}

/// Errors from API misuse.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReactiveError {
    /// The node was disposed (or never existed).
    #[error("node {0:?} is disposed")]
    DeadNode(NodeId),
    /// The scope was disposed (or never existed).
    #[error("scope {0:?} is disposed")]
    DeadScope(ScopeId),
    /// The operation needs a property but the node is an effect.
    #[error("node {0:?} is not a property")]
    NotAProperty(NodeId),
    /// The override is not on this property's stack.
    #[error("override {1:?} is not on the stack of {0:?}")]
    UnknownOverride(NodeId, OverrideId),
    /// The root scope lives as long as the graph.
    #[error("the root scope cannot be disposed or moved")]
    RootScope,
    /// `parent` is `scope` or inside its subtree.
    #[error("scope {parent:?} is inside {scope:?}")]
    ScopeCycle { scope: ScopeId, parent: ScopeId },
}

/// Graph-wide configuration.
#[derive(Clone, Copy, Debug)]
pub struct Options<V> {
    /// Change detection. A write or re-evaluation that produces an equal
    /// value does not notify observers.
    pub equals: fn(&V, &V) -> bool,
    /// Evaluate every binding eagerly (the `--eager` debug mode used for
    /// differential testing against Qt). Observable results must not change.
    pub eager: bool,
    /// Per-node cap on effect re-runs within one flush (handler ping-pong).
    pub max_reruns_per_flush: u32,
    /// How many undelivered diagnostics are kept when no handler is set.
    pub max_retained_diagnostics: usize,
}

impl<V: PartialEq> Default for Options<V> {
    fn default() -> Self {
        Options {
            equals: <V as PartialEq>::eq,
            eager: false,
            max_reruns_per_flush: 100,
            max_retained_diagnostics: 1024,
        }
    }
}

/// Counters for tests, benchmarks and the dev overlay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Binding evaluations.
    pub evaluations: u64,
    /// Effect runs.
    pub effect_runs: u64,
    /// Outermost flushes that ran at least one node.
    pub flushes: u64,
    /// Writes and notifications that propagated (value actually changed).
    pub propagations: u64,
    /// Runs whose edges were rebuilt: what they read was not what their
    /// last run read, in the same order.
    pub source_rebuilds: u64,
    /// Reads recorded as dependencies: the first read of each node by
    /// each run.
    pub reads: u64,
}

/// What the live nodes are, by kind: what a budget test pins, so a change
/// that moves bookkeeping out of the graph shows up as a number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Census {
    /// Properties whose top layer is a value.
    pub values: usize,
    /// Properties whose top layer is a binding.
    pub bindings: usize,
    /// Effects, told ones included.
    pub effects: usize,
    /// Live scopes, the root included.
    pub scopes: usize,
}
