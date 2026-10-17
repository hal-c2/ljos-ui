//! The graph itself. See the crate docs for the algorithm.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fmt;

use slotmap::SlotMap;
use smallvec::SmallVec;

use crate::types::{
    BindingFn, Diagnostic, EffectFn, NodeId, NodeKind, NodeState, Options, OverrideId,
    OverrideValue, Provenance, ReactiveError, Reader, RestoreMode, ScopeId, Stats, WriteKind,
};

/// One end of a dependency edge, stored on both nodes.
///
/// `back` is the index of the mirror entry in the other node's list: for an
/// entry in `sources` it is the position of this node inside the source's
/// `observers`, and the other way round. Keeping both indices current lets
/// an edge be removed with two swap-removes instead of a search, so
/// disposing a node costs the same whether its source has ten observers or
/// a hundred thousand.
#[derive(Clone, Copy)]
struct Edge {
    node: NodeId,
    back: u32,
}

type Edges = SmallVec<[Edge; 4]>;
type DiagnosticHandler = Box<dyn FnMut(&Diagnostic)>;

/// One layer of a property: the base, or an override above it.
struct Layer<V> {
    id: OverrideId,
    content: Content<V>,
    provenance: Provenance,
}

enum Content<V> {
    /// A plain value. While the layer is on top the value lives in
    /// `Node::value` (`None` here); a covered layer keeps its own copy so it
    /// can be restored.
    Value(Option<V>),
    /// A binding. The closure is `None` only while it runs.
    Binding(Option<BindingFn<V>>),
}

struct Prop<V> {
    base: Layer<V>,
    /// Usually empty, so it costs no allocation.
    overrides: Vec<Layer<V>>,
    animated: bool,
}

impl<V> Prop<V> {
    fn top(&self) -> &Layer<V> {
        self.overrides.last().unwrap_or(&self.base)
    }

    fn top_mut(&mut self) -> &mut Layer<V> {
        self.overrides.last_mut().unwrap_or(&mut self.base)
    }

    fn is_bound(&self) -> bool {
        matches!(self.top().content, Content::Binding(_))
    }
}

enum Kind<V> {
    Property(Prop<V>),
    /// The closure is `None` only while it runs.
    Effect(Option<EffectFn<V>>),
}

enum Compute<V> {
    Binding(BindingFn<V>),
    Effect(EffectFn<V>),
}

struct Node<V> {
    kind: Kind<V>,
    /// `Some` for properties, `None` for effects.
    value: Option<V>,
    state: NodeState,
    /// Marked while computing: run again afterwards.
    pending: bool,
    /// Promoted (or explicitly made) an effect root.
    eager: bool,
    queued: bool,
    /// The flush level its queue entry sits on (see `Inner::queues`).
    queued_level: u32,
    /// Parked on a frozen scope until thaw.
    deferred: bool,
    /// On the current check-walk stack; meeting it again is a cycle.
    walking: bool,
    /// Read itself in the run that is in progress.
    self_read: bool,
    /// The change this node's last run produced came back to it through
    /// its observers: it sits on a dependency cycle and has to settle.
    cyclic: bool,
    /// Its cycle is being settled: marks that reach it are dropped, which
    /// is what breaks the loop.
    settling: bool,
    /// While it settles: a source of it took a new value, so its change
    /// came back round as a change. That is the binding loop; a cycle whose
    /// other members give the values they had is none.
    came_back: bool,
    /// A binding set aside by [`Graph::hold`]: installed, not run.
    held: bool,
    height: u32,
    /// Creation order; breaks height ties so flush order is deterministic.
    seq: u64,
    scope: ScopeId,
    sources: Edges,
    observers: Edges,
    /// Scratch stamp for source diffing.
    mark: u64,
    /// Scratch for source diffing: where this node's observer entry sat
    /// before the diff started.
    scratch: u32,
    /// Opaque caller tag; the engine maps nodes back to (object, property).
    data: Option<u64>,
    /// Stamp of the last frame that recorded a read of this node.
    read_stamp: u64,
    /// Stamp of the last run whose reads became its sources.
    done_stamp: u64,
    runs: u32,
    run_epoch: u64,
    /// Bumped whenever the active layer changes, so a closure that was
    /// running when its binding was replaced is not put back.
    binding_gen: u32,
    /// Something asked whether the property was given a value at all
    /// ([`Graph::provenance_tracked`]): its observers are told when that
    /// changes, also when the value stays what it was.
    asked_if_set: bool,
}

impl<V> Node<V> {
    fn is_eager(&self, all: bool) -> bool {
        match &self.kind {
            Kind::Effect(_) => true,
            Kind::Property(p) => self.eager || (all && p.is_bound()),
        }
    }
}

struct Scope {
    parent: Option<ScopeId>,
    /// Children form an intrusive doubly linked list in creation order, so a
    /// child unlinks itself in O(1) and disposal order stays "newest first".
    first_child: Option<ScopeId>,
    last_child: Option<ScopeId>,
    prev: Option<ScopeId>,
    next: Option<ScopeId>,
    nodes: Vec<NodeId>,
    /// Dead entries in `nodes`; compacted when they dominate.
    dead: usize,
    frozen_self: bool,
    /// Effective: this scope or an ancestor is frozen.
    frozen: bool,
    deferred: Vec<NodeId>,
    cleanups: Vec<Box<dyn FnOnce()>>,
}

impl Scope {
    fn new(parent: Option<ScopeId>, frozen: bool) -> Self {
        Scope {
            parent,
            first_child: None,
            last_child: None,
            prev: None,
            next: None,
            nodes: Vec::new(),
            dead: 0,
            frozen_self: false,
            frozen,
            deferred: Vec::new(),
            cleanups: Vec::new(),
        }
    }
}

struct Frame {
    /// `None` for an `untrack` frame.
    node: Option<NodeId>,
    stamp: u64,
    reads: Vec<NodeId>,
}

struct Inner<V> {
    nodes: SlotMap<NodeId, Node<V>>,
    scopes: SlotMap<ScopeId, Scope>,
    root: ScopeId,
    /// Queued roots, one heap per flush level. A flush nested in an effect
    /// (a write it makes) runs every root that waits, so what the writer
    /// reads next is up to date. Each such root can write and nest once
    /// more, so past [`SHARED_FLUSH_DEPTH`] an effect that runs opens a
    /// level of its own and a flush nested in it drains only what that run
    /// queued. The call stack then grows with the chain of changes, as
    /// Qt's handlers nest, and not with the number of waiting roots.
    queues: Vec<Queue>,
    frames: Vec<Frame>,
    read_pool: Vec<Vec<NodeId>>,
    walk_pool: Vec<Vec<(NodeId, u32)>>,
    /// (node, colour, read stamp of the node that sent the mark).
    mark_stack: Vec<(NodeId, NodeState, u64)>,
    /// Nodes currently computing, outermost first.
    eval_stack: Vec<NodeId>,
    /// Work put off until a node that was computing has its value, with
    /// the node it waits for.
    after: Vec<(NodeId, Box<dyn FnOnce()>)>,
    /// The node whose freshly computed value is being announced. A mark
    /// that travels back to it closes a dependency cycle.
    wave_origin: Option<NodeId>,
    batch_depth: u32,
    flush_depth: u32,
    /// Scopes to freeze once the work under way is done (`set_frozen`).
    freeze_later: Vec<ScopeId>,
    flush_epoch: u64,
    tick: u64,
    seq: u64,
    next_override: u64,
    options: Options<V>,
    stats: Stats,
    diagnostics: Vec<Diagnostic>,
    dropped_diagnostics: usize,
    handler: Option<DiagnosticHandler>,
}

/// A single-threaded push-pull reactive graph over values of type `V`.
///
/// All methods take `&self`: binding and effect closures receive the same
/// `&Graph` and may read, write and create nodes re-entrantly. Internal state
/// sits in one `RefCell` that is never borrowed while user code runs.
pub struct Graph<V> {
    inner: RefCell<Inner<V>>,
}

impl<V> fmt::Debug for Graph<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.inner.try_borrow() {
            Ok(i) => f
                .debug_struct("Graph")
                .field("nodes", &i.nodes.len())
                .field("scopes", &i.scopes.len())
                .field("stats", &i.stats)
                .finish_non_exhaustive(),
            Err(_) => f.write_str("Graph { <busy> }"),
        }
    }
}

impl<V: Clone + PartialEq + 'static> Default for Graph<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Clone + PartialEq + 'static> Graph<V> {
    /// A lazy graph that compares values with `PartialEq`.
    pub fn new() -> Self {
        Self::with_options(Options::default())
    }
}

/// Runs on every exit from a computation, including unwinding, so a
/// panicking binding leaves the graph consistent and retryable.
struct ComputeGuard<'g, V: Clone + 'static> {
    graph: &'g Graph<V>,
    node: NodeId,
    gen_: u32,
    compute: Option<Compute<V>>,
    value: Option<V>,
    ok: bool,
}

impl<V: Clone + 'static> Drop for ComputeGuard<'_, V> {
    fn drop(&mut self) {
        let compute = self.compute.take();
        let value = self.value.take();
        self.graph
            .finish(self.node, self.gen_, compute, value, self.ok);
    }
}

struct BatchGuard<'g, V: Clone + 'static>(&'g Graph<V>);

impl<V: Clone + 'static> Drop for BatchGuard<'_, V> {
    fn drop(&mut self) {
        let outermost = {
            let mut i = self.0.inner.borrow_mut();
            i.batch_depth -= 1;
            i.batch_depth == 0
        };
        if outermost && !std::thread::panicking() {
            self.0.flush();
            self.0.freeze_waiting();
        }
    }
}

/// Closes the flush level an effect's run opened, on every exit. What is
/// still queued on it waits on the level below.
struct LevelGuard<'g, V: Clone + 'static>(&'g Graph<V>);

impl<V: Clone + 'static> Drop for LevelGuard<'_, V> {
    fn drop(&mut self) {
        let mut g = self.0.inner.borrow_mut();
        let i = &mut *g;
        let Some(left) = i.queues.pop() else {
            return;
        };
        let below = i.queues.len().saturating_sub(1);
        if i.queues.is_empty() {
            i.queues.push(Queue::new());
        }
        for Reverse(entry) in left {
            let n = entry.2;
            if let Some(node) = i.nodes.get_mut(n)
                && node.queued
                && node.queued_level as usize == below + 1
            {
                node.queued_level = below as u32;
                i.queues[below].push(Reverse(entry));
            }
        }
    }
}

struct FlushGuard<'g, V: Clone + 'static>(&'g Graph<V>);

impl<V: Clone + 'static> Drop for FlushGuard<'_, V> {
    fn drop(&mut self) {
        let idle = {
            let mut i = self.0.inner.borrow_mut();
            i.flush_depth -= 1;
            i.flush_depth == 0 && i.batch_depth == 0
        };
        if idle && !std::thread::panicking() {
            self.0.freeze_waiting();
        }
    }
}

/// Ends a node's settling on every exit, including unwinding.
struct SettleGuard<'g, V: Clone + 'static>(&'g Graph<V>, NodeId);

impl<V: Clone + 'static> Drop for SettleGuard<'_, V> {
    fn drop(&mut self) {
        if let Some(node) = self.0.inner.borrow_mut().nodes.get_mut(self.1) {
            node.settling = false;
        }
    }
}

struct UntrackGuard<'g, V: Clone + 'static>(&'g Graph<V>);

impl<V: Clone + 'static> Drop for UntrackGuard<'_, V> {
    fn drop(&mut self) {
        self.0.inner.borrow_mut().frames.pop();
    }
}

/// Disposes its scope when dropped.
pub struct ScopeHandle<'g, V: Clone + 'static> {
    graph: &'g Graph<V>,
    id: ScopeId,
}

impl<V: Clone + 'static> ScopeHandle<'_, V> {
    /// The scope this handle owns.
    pub fn id(&self) -> ScopeId {
        self.id
    }
}

impl<V: Clone + 'static> fmt::Debug for ScopeHandle<'_, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ScopeHandle").field(&self.id).finish()
    }
}

impl<V: Clone + 'static> Drop for ScopeHandle<'_, V> {
    fn drop(&mut self) {
        // Already disposed by hand is fine.
        let _ = self.graph.dispose_scope(self.id);
    }
}

impl<V: Clone + 'static> Graph<V> {
    /// A graph with explicit options (custom equality, `--eager` mode, caps).
    pub fn with_options(options: Options<V>) -> Self {
        let mut scopes = SlotMap::with_key();
        let root = scopes.insert(Scope::new(None, false));
        Graph {
            inner: RefCell::new(Inner {
                nodes: SlotMap::with_key(),
                scopes,
                root,
                queues: vec![Queue::new()],
                frames: Vec::new(),
                read_pool: Vec::new(),
                walk_pool: Vec::new(),
                mark_stack: Vec::new(),
                eval_stack: Vec::new(),
                after: Vec::new(),
                wave_origin: None,
                batch_depth: 0,
                flush_depth: 0,
                freeze_later: Vec::new(),
                flush_epoch: 0,
                tick: 0,
                seq: 0,
                next_override: 1,
                options,
                stats: Stats::default(),
                diagnostics: Vec::new(),
                dropped_diagnostics: 0,
                handler: None,
            }),
        }
    }

    // ----------------------------------------------------------------- scopes

    /// The root scope. It lives as long as the graph.
    pub fn root(&self) -> ScopeId {
        self.inner.borrow().root
    }

    /// Creates a child scope. It starts frozen if `parent` is frozen.
    pub fn create_scope(&self, parent: ScopeId) -> Result<ScopeId, ReactiveError> {
        let mut i = self.inner.borrow_mut();
        let frozen = i
            .scopes
            .get(parent)
            .ok_or(ReactiveError::DeadScope(parent))?
            .frozen;
        let id = i.scopes.insert(Scope::new(Some(parent), frozen));
        let tail = i.scopes[parent].last_child.replace(id);
        i.scopes[id].prev = tail;
        match tail {
            Some(t) => i.scopes[t].next = Some(id),
            None => i.scopes[parent].first_child = Some(id),
        }
        Ok(id)
    }

    /// Creates a child scope that is disposed when the handle drops.
    pub fn scope_handle(&self, parent: ScopeId) -> Result<ScopeHandle<'_, V>, ReactiveError> {
        Ok(ScopeHandle {
            graph: self,
            id: self.create_scope(parent)?,
        })
    }

    /// Whether the scope still exists.
    pub fn scope_alive(&self, scope: ScopeId) -> bool {
        self.inner.borrow().scopes.contains_key(scope)
    }

    /// Registers a callback that runs when the scope is disposed, after its
    /// child scopes and before its own nodes. Callbacks run in reverse
    /// registration order.
    pub fn on_dispose(
        &self,
        scope: ScopeId,
        f: impl FnOnce() + 'static,
    ) -> Result<(), ReactiveError> {
        let mut i = self.inner.borrow_mut();
        let s = i
            .scopes
            .get_mut(scope)
            .ok_or(ReactiveError::DeadScope(scope))?;
        s.cleanups.push(Box::new(f));
        Ok(())
    }

    /// Disposes a scope: child scopes first (newest first), then the
    /// scope's cleanups, then its nodes (newest first). Observers outside
    /// the scope are marked dirty and re-run in one flush.
    pub fn dispose_scope(&self, scope: ScopeId) -> Result<(), ReactiveError> {
        {
            let i = self.inner.borrow();
            if scope == i.root {
                return Err(ReactiveError::RootScope);
            }
            if !i.scopes.contains_key(scope) {
                return Err(ReactiveError::DeadScope(scope));
            }
        }
        self.batch(|| self.dispose_scope_inner(scope));
        Ok(())
    }

    fn dispose_scope_inner(&self, scope: ScopeId) {
        // A snapshot: a cleanup may dispose or add siblings while we walk.
        let children = {
            let i = self.inner.borrow();
            let Some(s) = i.scopes.get(scope) else {
                return;
            };
            let mut children = Vec::new();
            let mut cur = s.first_child;
            while let Some(c) = cur {
                children.push(c);
                cur = i.scopes[c].next;
            }
            children
        };
        for child in children.into_iter().rev() {
            self.dispose_scope_inner(child);
        }
        let cleanups = match self.inner.borrow_mut().scopes.get_mut(scope) {
            Some(s) => std::mem::take(&mut s.cleanups),
            None => return,
        };
        for cleanup in cleanups.into_iter().rev() {
            cleanup();
        }
        let nodes = match self.inner.borrow_mut().scopes.get_mut(scope) {
            Some(s) => std::mem::take(&mut s.nodes),
            None => return,
        };
        for n in nodes.into_iter().rev() {
            // Nodes disposed one by one earlier are already gone.
            let _ = self.dispose(n);
        }
        let leftover = {
            let mut i = self.inner.borrow_mut();
            match i.scopes.remove(scope) {
                Some(s) => {
                    i.unlink_scope(&s);
                    s.nodes
                }
                None => Vec::new(),
            }
        };
        // Nodes a cleanup created in this scope go too.
        for n in leftover.into_iter().rev() {
            let _ = self.dispose(n);
        }
    }

    /// Freezes or thaws a scope (ARCHITECTURE.md §8.3). While a scope is
    /// frozen, effects it or its descendants own are parked instead of run;
    /// repeated changes coalesce, and on thaw each parked effect runs once
    /// against the latest values. Bindings are unaffected: they stay lazy
    /// and still evaluate when read.
    ///
    /// Freezing stops work to come, not work already caused: asked for
    /// inside a batch or while effects run, it takes hold once those
    /// effects have run. A page hidden and frozen in one statement still
    /// sees itself hidden. Thawing is at once.
    pub fn set_frozen(&self, scope: ScopeId, frozen: bool) -> Result<(), ReactiveError> {
        {
            let mut g = self.inner.borrow_mut();
            if !g.scopes.contains_key(scope) {
                return Err(ReactiveError::DeadScope(scope));
            }
            g.freeze_later.retain(|s| *s != scope);
            if frozen && (g.batch_depth > 0 || g.flush_depth > 0) {
                g.freeze_later.push(scope);
                return Ok(());
            }
            g.scopes[scope].frozen_self = frozen;
            g.refreeze(scope);
        }
        self.flush_if_idle();
        Ok(())
    }

    /// Freezes the scopes `set_frozen` put off, now that nothing runs.
    fn freeze_waiting(&self) {
        let mut g = self.inner.borrow_mut();
        let i = &mut *g;
        for scope in std::mem::take(&mut i.freeze_later) {
            if let Some(s) = i.scopes.get_mut(scope) {
                s.frozen_self = true;
                i.refreeze(scope);
            }
        }
    }

    /// Moves `scope` (with its subtree) to the end of `new_parent`'s
    /// children, so it is disposed with `new_parent` from now on. O(1) for
    /// the move itself; finding a cycle walks `new_parent`'s ancestors, and
    /// a change of the effective frozen state walks the subtree.
    ///
    /// Fails for the root, a disposed scope, or a `new_parent` inside
    /// `scope`'s subtree.
    pub fn reparent_scope(&self, scope: ScopeId, new_parent: ScopeId) -> Result<(), ReactiveError> {
        {
            let mut g = self.inner.borrow_mut();
            let i = &mut *g;
            if scope == i.root {
                return Err(ReactiveError::RootScope);
            }
            let old_parent = i
                .scopes
                .get(scope)
                .ok_or(ReactiveError::DeadScope(scope))?
                .parent;
            if !i.scopes.contains_key(new_parent) {
                return Err(ReactiveError::DeadScope(new_parent));
            }
            let mut cur = Some(new_parent);
            while let Some(c) = cur {
                if c == scope {
                    return Err(ReactiveError::ScopeCycle {
                        scope,
                        parent: new_parent,
                    });
                }
                cur = i.scopes[c].parent;
            }
            if old_parent == Some(new_parent) && i.scopes[new_parent].last_child == Some(scope) {
                return Ok(());
            }
            let (prev, next) = (i.scopes[scope].prev, i.scopes[scope].next);
            i.unlink_links(old_parent, prev, next);
            let tail = i.scopes[new_parent].last_child.replace(scope);
            {
                let s = &mut i.scopes[scope];
                s.parent = Some(new_parent);
                s.prev = tail;
                s.next = None;
            }
            match tail {
                Some(t) => i.scopes[t].next = Some(scope),
                None => i.scopes[new_parent].first_child = Some(scope),
            }
            let was = i.scopes[scope].frozen;
            let now = i.scopes[new_parent].frozen || i.scopes[scope].frozen_self;
            if was != now {
                i.refreeze(scope);
            }
        }
        self.flush_if_idle();
        Ok(())
    }

    /// The scope's parent; `None` for the root or a disposed scope.
    pub fn scope_parent(&self, scope: ScopeId) -> Option<ScopeId> {
        self.inner.borrow().scopes.get(scope)?.parent
    }

    /// Whether the scope is frozen, directly or through an ancestor.
    pub fn is_frozen(&self, scope: ScopeId) -> bool {
        self.inner
            .borrow()
            .scopes
            .get(scope)
            .is_some_and(|s| s.frozen)
    }

    // ------------------------------------------------------------ creation

    /// Creates a plain property with provenance [`Provenance::Default`].
    pub fn property(&self, scope: ScopeId, value: V) -> Result<NodeId, ReactiveError> {
        self.property_with(scope, value, Provenance::Default)
    }

    /// Creates a plain property whose value already has a history: a host
    /// that makes nodes on demand uses it for a property that was assigned
    /// before anything observed it. Unlike a [`WriteKind::Initial`] write
    /// after [`Graph::property`] it never flushes, so it is safe in the
    /// middle of a binding's evaluation.
    pub fn property_with(
        &self,
        scope: ScopeId,
        value: V,
        provenance: Provenance,
    ) -> Result<NodeId, ReactiveError> {
        let mut i = self.inner.borrow_mut();
        i.insert_node(
            scope,
            Kind::Property(Prop {
                base: Layer {
                    id: OverrideId(0),
                    content: Content::Value(None),
                    provenance,
                },
                overrides: Vec::new(),
                animated: false,
            }),
            Some(value),
            NodeState::Clean,
        )
    }

    /// Creates a bound property. It is lazy: `f` first runs when the value
    /// is read (or at the next flush in eager mode). `initial` is what a
    /// binding loop sees before the first evaluation finishes.
    pub fn binding(
        &self,
        scope: ScopeId,
        initial: V,
        f: impl FnMut(&Graph<V>) -> V + 'static,
    ) -> Result<NodeId, ReactiveError> {
        let n = {
            let mut i = self.inner.borrow_mut();
            let n = i.insert_node(
                scope,
                Kind::Property(Prop {
                    base: Layer {
                        id: OverrideId(0),
                        content: Content::Binding(Some(Box::new(f))),
                        provenance: Provenance::Bound,
                    },
                    overrides: Vec::new(),
                    animated: false,
                }),
                Some(initial),
                NodeState::Dirty,
            )?;
            if i.options.eager {
                let Inner { nodes, queues, .. } = &mut *i;
                enqueue(queues, n, &mut nodes[n]);
            }
            n
        };
        self.flush_if_idle();
        Ok(n)
    }

    /// Creates an effect root. It runs once now (or when the current batch
    /// ends, or on thaw), then again whenever something it read changes.
    /// Effects run synchronously in height order before the write that
    /// triggered them returns.
    pub fn effect(
        &self,
        scope: ScopeId,
        f: impl FnMut(&Graph<V>) + 'static,
    ) -> Result<NodeId, ReactiveError> {
        let n = {
            let mut i = self.inner.borrow_mut();
            let n = i.insert_node(
                scope,
                Kind::Effect(Some(Box::new(f))),
                None,
                NodeState::Dirty,
            )?;
            let Inner { nodes, queues, .. } = &mut *i;
            enqueue(queues, n, &mut nodes[n]);
            n
        };
        self.flush_if_idle();
        Ok(n)
    }

    /// Subscribes to changes of one property, like `onXChanged` or a Rust
    /// subscriber. Unlike [`Graph::effect`] it does not run on creation, and
    /// reads inside `handler` are not tracked. Returns the effect node;
    /// dispose it to unsubscribe.
    pub fn watch(
        &self,
        scope: ScopeId,
        target: NodeId,
        mut handler: impl FnMut(&Graph<V>, &V) + 'static,
    ) -> Result<NodeId, ReactiveError> {
        // The target must be clean before we link to it, or a later change
        // would stop at the already-dirty target and never reach us.
        self.try_get_untracked(target)?;
        let mut g = self.inner.borrow_mut();
        let f = move |g: &Graph<V>| {
            if let Some(v) = g.try_get(target) {
                g.untrack(|| handler(g, &v));
            }
        };
        let n = g.insert_node(
            scope,
            Kind::Effect(Some(Box::new(f))),
            None,
            NodeState::Clean,
        )?;
        let i = &mut *g;
        let h = i.nodes[target].height;
        // `n` is new, so the target is its only source: index 0.
        i.nodes[target].observers.push(Edge { node: n, back: 0 });
        let at = i.nodes[target].observers.len() as u32 - 1;
        let node = &mut i.nodes[n];
        node.sources.push(Edge {
            node: target,
            back: at,
        });
        node.height = h + 1;
        Ok(n)
    }

    /// Disposes one node. Its observers are marked dirty: their next run
    /// reads a dead node (`try_get` returns `None`) and drops the edge.
    pub fn dispose(&self, n: NodeId) -> Result<(), ReactiveError> {
        let node = {
            let mut g = self.inner.borrow_mut();
            let i = &mut *g;
            let node = i.nodes.remove(n).ok_or(ReactiveError::DeadNode(n))?;
            for e in &node.sources {
                drop_observer_slot(&mut i.nodes, e.node, e.back);
            }
            let mut stack = std::mem::take(&mut i.mark_stack);
            for e in &node.observers {
                if drop_source_slot(&mut i.nodes, e.node, e.back).is_some() {
                    stack.push((e.node, NodeState::Dirty, ALWAYS_STALE));
                }
            }
            i.mark_stack = stack;
            i.propagate_marks();
            if let Some(s) = i.scopes.get_mut(node.scope) {
                s.dead += 1;
                if s.dead * 2 > s.nodes.len() {
                    let nodes = &i.nodes;
                    s.nodes.retain(|id| nodes.contains_key(*id));
                    s.dead = 0;
                }
            }
            node
        };
        // User closures may own graph handles; drop them unborrowed.
        drop(node);
        self.flush_if_idle();
        Ok(())
    }

    // --------------------------------------------------------------- reads

    /// Reads a property, evaluating it if needed and recording the
    /// dependency when called from a binding or effect.
    ///
    /// # Panics
    ///
    /// If `n` is disposed or is an effect. Use [`Graph::try_get`] where the
    /// node may be gone.
    pub fn get(&self, n: NodeId) -> V {
        match self.try_get(n) {
            Some(v) => v,
            None => panic!("ljos_reactive: read of disposed or non-property node {n:?}"),
        }
    }

    /// Like [`Graph::get`], but `None` for a dead node or an effect.
    pub fn try_get(&self, n: NodeId) -> Option<V> {
        {
            let mut g = self.inner.borrow_mut();
            let node = g.nodes.get(n)?;
            node.value.as_ref()?;
            if node.state == NodeState::Clean {
                g.record_read(n);
                return g.nodes[n].value.clone();
            }
        }
        self.read_slow(n)
    }

    /// Reads without recording a dependency (still evaluates if dirty).
    pub fn get_untracked(&self, n: NodeId) -> V {
        self.untrack(|| self.get(n))
    }

    /// The value the property holds right now: nothing is evaluated and no
    /// dependency is recorded, so a dirty binding gives its last result
    /// (or the value it was installed over). `None` for a dead node or an
    /// effect.
    pub fn peek(&self, n: NodeId) -> Option<V> {
        self.inner.borrow().nodes.get(n)?.value.clone()
    }

    fn try_get_untracked(&self, n: NodeId) -> Result<V, ReactiveError> {
        {
            let i = self.inner.borrow();
            let node = i.nodes.get(n).ok_or(ReactiveError::DeadNode(n))?;
            if node.value.is_none() {
                return Err(ReactiveError::NotAProperty(n));
            }
        }
        self.untrack(|| self.try_get(n))
            .ok_or(ReactiveError::DeadNode(n))
    }

    #[cold]
    fn read_slow(&self, n: NodeId) -> Option<V> {
        // Update before recording the read: marks that `n` sends to the
        // reader while it catches up are not news to the reader.
        self.update(n);
        let v = {
            let mut i = self.inner.borrow_mut();
            i.nodes.contains_key(n).then(|| i.record_read(n));
            i.nodes.get(n)?.value.clone()
        };
        self.dispatch_diagnostics();
        v
    }

    /// Runs `f` without recording reads as dependencies of the running
    /// binding or effect.
    pub fn untrack<R>(&self, f: impl FnOnce() -> R) -> R {
        self.inner.borrow_mut().frames.push(Frame {
            node: None,
            stamp: 0,
            reads: Vec::new(),
        });
        let _guard = UntrackGuard(self);
        f()
    }

    /// Whether a read right now would be recorded as a dependency: true
    /// while a binding or effect evaluates, false at top level and inside
    /// [`Graph::untrack`]. Callers use it to skip work (creating a node for
    /// a property that nothing listens to) when no one is observing.
    pub fn is_tracking(&self) -> bool {
        self.inner
            .borrow()
            .frames
            .last()
            .is_some_and(|f| f.node.is_some())
    }

    /// The run that a read right now belongs to: `None` where
    /// [`Graph::is_tracking`] is false. For a caller that keeps what was
    /// read without a node, and makes the node only when it changes.
    pub fn reader(&self) -> Option<Reader> {
        let i = self.inner.borrow();
        let f = i.frames.last()?;
        Some(Reader {
            node: f.node?,
            run: f.stamp,
        })
    }

    /// Whether what `r` read still counts: the run is in progress, or it
    /// is the reader's last one and its reads are the reader's sources.
    pub fn is_current(&self, r: Reader) -> bool {
        self.inner.borrow().is_current(r)
    }

    /// Makes `n` a source of the readers that read what it stands for, as
    /// if they had read `n`: a run in progress gets it among its reads, a
    /// finished one as a source, and the next change of `n` reaches them.
    /// Readers that are not [current](Graph::is_current) are passed over.
    /// For a node made after the reads, for a value that had none.
    pub fn adopt_readers(&self, n: NodeId, readers: impl IntoIterator<Item = Reader>) {
        let mut g = self.inner.borrow_mut();
        let i = &mut *g;
        if !i.nodes.contains_key(n) {
            return;
        }
        for r in readers {
            if r.node == n {
                continue;
            }
            if let Some(f) = i.frames.iter_mut().rev().find(|f| f.node == Some(r.node))
                && f.stamp == r.run
            {
                if i.nodes[n].read_stamp != r.run {
                    i.nodes[n].read_stamp = r.run;
                    f.reads.push(n);
                }
                continue;
            }
            let Some(reader) = i.nodes.get(r.node) else {
                continue;
            };
            if reader.done_stamp != r.run || i.nodes[n].observers.iter().any(|e| e.node == r.node) {
                continue;
            }
            let back = reader.sources.len() as u32;
            let slot = i.nodes[n].observers.len() as u32;
            let height = i.nodes[n].height + 1;
            let reader = &mut i.nodes[r.node];
            reader.height = reader.height.max(height);
            reader.sources.push(Edge {
                node: n,
                back: slot,
            });
            i.nodes[n].observers.push(Edge { node: r.node, back });
        }
    }

    /// What [`Graph::dispose`] does to the observers of a node, for reads
    /// that never had one: each reader whose last finished run made the
    /// read is marked dirty, and finds what it read gone when it runs.
    pub fn readers_lost(&self, readers: impl IntoIterator<Item = Reader>) {
        {
            let mut g = self.inner.borrow_mut();
            let i = &mut *g;
            let mut stack = std::mem::take(&mut i.mark_stack);
            for r in readers {
                if i.nodes.get(r.node).is_some_and(|n| n.done_stamp == r.run) {
                    stack.push((r.node, NodeState::Dirty, ALWAYS_STALE));
                }
            }
            if stack.is_empty() {
                i.mark_stack = stack;
                return;
            }
            i.mark_stack = stack;
            i.propagate_marks();
        }
        self.flush_if_idle();
    }

    // -------------------------------------------------------------- writes

    /// An imperative write (`x = v` from JS): breaks the binding on the top
    /// layer, sets provenance to [`Provenance::Assigned`], notifies if the
    /// value changed and flushes effects before returning (unless batched).
    ///
    /// # Panics
    ///
    /// If `n` is disposed or an effect. See [`Graph::write`].
    pub fn set(&self, n: NodeId, v: V) {
        if let Err(e) = self.write(n, v, WriteKind::Imperative) {
            panic!("ljos_reactive: {e}");
        }
    }

    /// Writes with explicit binding semantics; see [`WriteKind`].
    pub fn write(&self, n: NodeId, v: V, how: WriteKind) -> Result<(), ReactiveError> {
        let dropped;
        {
            let mut g = self.inner.borrow_mut();
            let i = &mut *g;
            if !i.nodes.contains_key(n) {
                return Err(ReactiveError::DeadNode(n));
            }
            if !matches!(i.nodes[n].kind, Kind::Property(_)) {
                return Err(ReactiveError::NotAProperty(n));
            }
            // A binding that writes is side-effecting: laziness would make
            // the side effect depend on who reads it, so it becomes a root.
            if let Some(&running) = i.eval_stack.last()
                && running != n
                && let Some(r) = i.nodes.get_mut(running)
                && matches!(r.kind, Kind::Property(_))
            {
                r.eager = true;
            }
            let was_unset = i.unset(n);
            let Kind::Property(p) = &mut i.nodes[n].kind else {
                unreachable!()
            };
            match how {
                WriteKind::Imperative => {
                    let top = p.top_mut();
                    dropped = std::mem::replace(&mut top.content, Content::Value(None));
                    top.provenance = Provenance::Assigned;
                    p.animated = false;
                    i.make_plain(n);
                    i.set_value(n, v);
                }
                WriteKind::Initial => {
                    p.animated = false;
                    if p.overrides.is_empty() {
                        dropped = std::mem::replace(&mut p.base.content, Content::Value(None));
                        p.base.provenance = Provenance::Assigned;
                        i.make_plain(n);
                        i.set_value(n, v);
                    } else {
                        dropped = std::mem::replace(&mut p.base.content, Content::Value(Some(v)));
                        p.base.provenance = Provenance::Assigned;
                    }
                }
                WriteKind::Animation => {
                    dropped = Content::Value(None);
                    p.animated = true;
                    i.set_value(n, v);
                }
                WriteKind::Layout if p.is_bound() => {
                    // The binding and its edges stay; a run that was
                    // already due still comes and wins.
                    dropped = Content::Value(None);
                    i.set_value(n, v);
                }
                WriteKind::Layout => {
                    let top = p.top_mut();
                    dropped = std::mem::replace(&mut top.content, Content::Value(None));
                    top.provenance = Provenance::Assigned;
                    p.animated = false;
                    i.make_plain(n);
                    i.set_value(n, v);
                }
                WriteKind::Override(id) => {
                    let idx = p
                        .overrides
                        .iter()
                        .position(|l| l.id == id)
                        .ok_or(ReactiveError::UnknownOverride(n, id))?;
                    let is_top = idx + 1 == p.overrides.len();
                    let layer = &mut p.overrides[idx];
                    layer.provenance = Provenance::Assigned;
                    if is_top {
                        dropped = std::mem::replace(&mut layer.content, Content::Value(None));
                        p.animated = false;
                        i.make_plain(n);
                        i.set_value(n, v);
                    } else {
                        dropped = std::mem::replace(&mut layer.content, Content::Value(Some(v)));
                    }
                }
            }
            i.tell_if_set(n, was_unset);
        }
        drop(dropped);
        self.flush_if_idle();
        Ok(())
    }

    /// Takes a property back to a value nothing gave it (`x = undefined` on
    /// a property with a reset): as an imperative write, the binding on the
    /// top layer goes and observers are told if the value changed, but the
    /// provenance is [`Provenance::Default`] again, so what follows the
    /// property only while it is unset ([`Graph::provenance_tracked`])
    /// follows it again.
    pub fn reset(&self, n: NodeId, v: V) -> Result<(), ReactiveError> {
        let dropped = {
            let mut g = self.inner.borrow_mut();
            let i = &mut *g;
            if !i.nodes.contains_key(n) {
                return Err(ReactiveError::DeadNode(n));
            }
            let was_unset = i.unset(n);
            let Kind::Property(p) = &mut i.nodes[n].kind else {
                return Err(ReactiveError::NotAProperty(n));
            };
            let top = p.top_mut();
            let dropped = std::mem::replace(&mut top.content, Content::Value(None));
            top.provenance = Provenance::Default;
            p.animated = false;
            i.make_plain(n);
            i.set_value(n, v);
            i.tell_if_set(n, was_unset);
            dropped
        };
        // Outside the borrow: dropping a binding may run the embedder's code.
        drop(dropped);
        self.flush_if_idle();
        Ok(())
    }

    /// Changes the value of a plain property where it sits, and notifies as
    /// a write does. What it is for: a value that is costly to copy (a long
    /// list) and changes a little (one more item); [`Graph::write`] needs
    /// the whole new value, and making it costs a copy each time.
    ///
    /// `f` must change the value (observers are told it changed without a
    /// comparison) and must not use the graph. It runs only when the
    /// property holds a plain value with nothing over it: no binding, no
    /// override, not animated. Otherwise this returns `false` and the
    /// caller writes the usual way.
    pub fn modify(&self, n: NodeId, f: impl FnOnce(&mut V)) -> Result<bool, ReactiveError> {
        {
            let mut g = self.inner.borrow_mut();
            let i = &mut *g;
            let node = i.nodes.get_mut(n).ok_or(ReactiveError::DeadNode(n))?;
            let Kind::Property(p) = &node.kind else {
                return Err(ReactiveError::NotAProperty(n));
            };
            let plain = p.overrides.is_empty()
                && !p.animated
                && matches!(p.base.content, Content::Value(None))
                && node.state == NodeState::Clean
                && !node.held;
            let Some(v) = node.value.as_mut().filter(|_| plain) else {
                return Ok(false);
            };
            f(v);
            // As in `write`: a binding that changes another property is
            // side-effecting, so it may not stay lazy.
            if let Some(&running) = i.eval_stack.last()
                && running != n
                && let Some(r) = i.nodes.get_mut(running)
                && matches!(r.kind, Kind::Property(_))
            {
                r.eager = true;
            }
            if let Kind::Property(p) = &mut i.nodes[n].kind {
                p.base.provenance = Provenance::Assigned;
            }
            i.stats.propagations += 1;
            i.mark_observers(n, NodeState::Dirty);
        }
        self.flush_if_idle();
        Ok(true)
    }

    /// Changes what a plain property reads as and tells nobody. A later read
    /// finds the new value; what read the old one is not run again for it.
    ///
    /// What it is for: a value that stops being there rather than changes
    /// (a role of a model row that was removed). Qt has no change signal
    /// for that either, and a binding that read the value keeps its result.
    ///
    /// Only for a property that holds a plain value with nothing over it,
    /// as in [`Graph::modify`]; otherwise this returns `false`, nothing is
    /// changed, and the caller writes the usual way.
    pub fn replace_quietly(&self, n: NodeId, v: V) -> Result<bool, ReactiveError> {
        let old = {
            let mut g = self.inner.borrow_mut();
            let node = g.nodes.get_mut(n).ok_or(ReactiveError::DeadNode(n))?;
            let Kind::Property(p) = &node.kind else {
                return Err(ReactiveError::NotAProperty(n));
            };
            let plain = p.overrides.is_empty()
                && !p.animated
                && matches!(p.base.content, Content::Value(None))
                && node.state == NodeState::Clean
                && !node.held;
            if !plain || node.value.is_none() {
                return Ok(false);
            }
            node.value.replace(v)
        };
        // Outside the borrow: dropping a value may run the embedder's code.
        drop(old);
        Ok(true)
    }

    /// Installs a binding on the top layer (`x = Qt.binding(...)`), replacing
    /// whatever was there. The property becomes dirty; observers re-check.
    pub fn bind(
        &self,
        n: NodeId,
        f: impl FnMut(&Graph<V>) -> V + 'static,
    ) -> Result<(), ReactiveError> {
        let dropped = {
            let mut g = self.inner.borrow_mut();
            let i = &mut *g;
            let was_unset = i.unset(n);
            let node = i.nodes.get_mut(n).ok_or(ReactiveError::DeadNode(n))?;
            let Kind::Property(p) = &mut node.kind else {
                return Err(ReactiveError::NotAProperty(n));
            };
            p.animated = false;
            let top = p.top_mut();
            top.provenance = Provenance::Bound;
            let dropped = std::mem::replace(&mut top.content, Content::Binding(Some(Box::new(f))));
            i.make_bound_dirty(n);
            i.tell_if_set(n, was_unset);
            dropped
        };
        drop(dropped);
        self.flush_if_idle();
        Ok(())
    }

    /// Sets a binding that has not run yet aside: until [`Graph::release`]
    /// the property reads as the value the binding was installed over, and
    /// reading it does not run the binding. Does nothing unless `n` is a
    /// dirty binding.
    ///
    /// Qt installs the bindings of some properties after all others of a
    /// component have run; an engine that wants the same results (which
    /// member of a binding loop is reported, and what it is left with)
    /// holds those back with this.
    pub fn hold(&self, n: NodeId) -> Result<(), ReactiveError> {
        let mut i = self.inner.borrow_mut();
        let node = i.nodes.get_mut(n).ok_or(ReactiveError::DeadNode(n))?;
        let Kind::Property(p) = &node.kind else {
            return Err(ReactiveError::NotAProperty(n));
        };
        if p.is_bound() && node.state == NodeState::Dirty {
            node.state = NodeState::Clean;
            node.held = true;
        }
        Ok(())
    }

    /// Ends [`Graph::hold`]: the binding is dirty again and its observers
    /// re-check, as if it had been installed now. Does nothing if `n` is
    /// not held any more (it ran, or was written or bound since).
    pub fn release(&self, n: NodeId) -> Result<(), ReactiveError> {
        {
            let mut i = self.inner.borrow_mut();
            let node = i.nodes.get_mut(n).ok_or(ReactiveError::DeadNode(n))?;
            if !node.held {
                return Ok(());
            }
            i.make_bound_dirty(n);
        }
        self.flush_if_idle();
        Ok(())
    }

    /// Whether [`Graph::hold`] has set `n` aside.
    pub fn is_held(&self, n: NodeId) -> bool {
        self.inner.borrow().nodes.get(n).is_some_and(|x| x.held)
    }

    /// Forces a change notification (`emit xChanged()`): observers re-run
    /// even if the value is equal. This is also how an external revision
    /// counter (for example a translation revision) invalidates readers.
    pub fn notify(&self, n: NodeId) -> Result<(), ReactiveError> {
        {
            let mut i = self.inner.borrow_mut();
            if !i.nodes.contains_key(n) {
                return Err(ReactiveError::DeadNode(n));
            }
            i.stats.propagations += 1;
            i.mark_observers(n, NodeState::Dirty);
        }
        self.flush_if_idle();
        Ok(())
    }

    /// Defers the effect flush until `f` returns (Qt's property update
    /// group). Nested batches flush once, at the outermost end. Reads
    /// inside the batch pull current values.
    pub fn batch<R>(&self, f: impl FnOnce() -> R) -> R {
        self.inner.borrow_mut().batch_depth += 1;
        let _guard = BatchGuard(self);
        f()
    }

    /// Re-evaluates the active binding (or effect) in place: it becomes
    /// dirty and observers re-check, but provenance, the override stack and
    /// the installed closure are untouched. Hot reload and the engine use it
    /// where [`Graph::bind`] would reset provenance to bound. A plain value
    /// has nothing to re-run, so this does nothing. Called from inside the
    /// node's own evaluation it schedules one more run afterwards.
    pub fn refresh(&self, n: NodeId) -> Result<(), ReactiveError> {
        {
            let mut g = self.inner.borrow_mut();
            let i = &mut *g;
            let all = i.options.eager;
            let node = i.nodes.get_mut(n).ok_or(ReactiveError::DeadNode(n))?;
            let reruns = match &node.kind {
                Kind::Property(p) => p.is_bound(),
                Kind::Effect(_) => true,
            };
            if !reruns {
                return Ok(());
            }
            if node.state == NodeState::Computing {
                // `finish` re-dirties it.
                node.pending = true;
            } else {
                node.state = NodeState::Dirty;
                if node.is_eager(all) {
                    enqueue(&mut i.queues, n, node);
                }
                i.mark_observers(n, NodeState::Check);
            }
        }
        self.flush_if_idle();
        Ok(())
    }

    // ----------------------------------------------------------- overrides

    /// Pushes an override (`Binding {}`, `PropertyChanges`) above the
    /// property's current layers. The covered layer is kept for restore.
    pub fn push_override(
        &self,
        n: NodeId,
        value: OverrideValue<V>,
    ) -> Result<OverrideId, ReactiveError> {
        let id = {
            let mut g = self.inner.borrow_mut();
            let i = &mut *g;
            let id = OverrideId(i.next_override);
            let was_unset = i.unset(n);
            let node = i.nodes.get_mut(n).ok_or(ReactiveError::DeadNode(n))?;
            let Kind::Property(p) = &mut node.kind else {
                return Err(ReactiveError::NotAProperty(n));
            };
            i.next_override += 1;
            p.animated = false;
            if let Content::Value(saved) = &mut p.top_mut().content {
                *saved = node.value.clone();
            }
            match value {
                OverrideValue::Value(v) => {
                    p.overrides.push(Layer {
                        id,
                        content: Content::Value(None),
                        provenance: Provenance::Assigned,
                    });
                    i.make_plain(n);
                    i.set_value(n, v);
                }
                OverrideValue::Binding(f) => {
                    p.overrides.push(Layer {
                        id,
                        content: Content::Binding(Some(f)),
                        provenance: Provenance::Bound,
                    });
                    i.make_bound_dirty(n);
                }
            }
            i.tell_if_set(n, was_unset);
            id
        };
        self.flush_if_idle();
        Ok(id)
    }

    /// Removes an override. If it is on top, `mode` decides whether the
    /// layer below comes back or the current value stays (see
    /// [`RestoreMode`]). Removing a covered override just drops it.
    pub fn remove_override(
        &self,
        n: NodeId,
        id: OverrideId,
        mode: RestoreMode,
    ) -> Result<(), ReactiveError> {
        // "Keep the current value" needs the current value, which a lazy
        // binding may not have computed yet.
        self.try_get_untracked(n)?;
        let dropped = {
            let mut g = self.inner.borrow_mut();
            let i = &mut *g;
            let was_unset = i.unset(n);
            let node = i.nodes.get_mut(n).ok_or(ReactiveError::DeadNode(n))?;
            let Kind::Property(p) = &mut node.kind else {
                return Err(ReactiveError::NotAProperty(n));
            };
            let idx = p
                .overrides
                .iter()
                .position(|l| l.id == id)
                .ok_or(ReactiveError::UnknownOverride(n, id))?;
            let removed = p.overrides.remove(idx);
            if idx < p.overrides.len() {
                Some(removed)
            } else {
                p.animated = false;
                let below = p.top_mut();
                let below_bound = matches!(below.content, Content::Binding(_));
                let restore = match mode {
                    RestoreMode::BindingOrValue => true,
                    RestoreMode::Binding => below_bound,
                    RestoreMode::Value => !below_bound,
                    RestoreMode::None => false,
                };
                if !restore {
                    let old = std::mem::replace(&mut below.content, Content::Value(None));
                    below.provenance = Provenance::Assigned;
                    i.make_plain(n);
                    drop(old);
                } else if let Content::Value(saved) = &mut below.content {
                    let v = saved.take();
                    i.make_plain(n);
                    if let Some(v) = v {
                        i.set_value(n, v);
                    }
                } else {
                    i.make_bound_dirty(n);
                }
                i.tell_if_set(n, was_unset);
                Some(removed)
            }
        };
        drop(dropped);
        self.flush_if_idle();
        Ok(())
    }

    /// Number of active overrides on a property.
    pub fn override_count(&self, n: NodeId) -> usize {
        match self.inner.borrow().nodes.get(n).map(|n| &n.kind) {
            Some(Kind::Property(p)) => p.overrides.len(),
            _ => 0,
        }
    }

    // ------------------------------------------------------------ inspection

    /// Provenance of the current value: the top layer's, or
    /// [`Provenance::Animated`] after an animation write.
    pub fn provenance(&self, n: NodeId) -> Option<Provenance> {
        match &self.inner.borrow().nodes.get(n)?.kind {
            Kind::Property(p) if p.animated => Some(Provenance::Animated),
            Kind::Property(p) => Some(p.top().provenance),
            Kind::Effect(_) => None,
        }
    }

    /// [`Graph::provenance`] as a tracked read: what reads it runs again
    /// when the property stops being [`Provenance::Default`] or becomes it
    /// again, whether or not the value changes with that. For a default
    /// that means something else than the same value given on purpose (a
    /// margin that follows another until it is set).
    pub fn provenance_tracked(&self, n: NodeId) -> Option<Provenance> {
        let p = self.provenance(n)?;
        let mut g = self.inner.borrow_mut();
        g.nodes[n].asked_if_set = true;
        g.record_read(n);
        Some(p)
    }

    /// Provenance of the base layer, ignoring overrides and animation. Hot
    /// reload uses it to tell assigned state from bound state.
    pub fn base_provenance(&self, n: NodeId) -> Option<Provenance> {
        match &self.inner.borrow().nodes.get(n)?.kind {
            Kind::Property(p) => Some(p.base.provenance),
            Kind::Effect(_) => None,
        }
    }

    /// Whether the active layer is a binding.
    pub fn is_bound(&self, n: NodeId) -> bool {
        matches!(
            self.inner.borrow().nodes.get(n).map(|n| &n.kind),
            Some(Kind::Property(p)) if p.is_bound()
        )
    }

    /// Whether the node still exists.
    pub fn is_alive(&self, n: NodeId) -> bool {
        self.inner.borrow().nodes.contains_key(n)
    }

    /// Property or effect.
    pub fn kind(&self, n: NodeId) -> Option<NodeKind> {
        Some(match self.inner.borrow().nodes.get(n)?.kind {
            Kind::Property(_) => NodeKind::Property,
            Kind::Effect(_) => NodeKind::Effect,
        })
    }

    /// The node's colour.
    pub fn state(&self, n: NodeId) -> Option<NodeState> {
        Some(self.inner.borrow().nodes.get(n)?.state)
    }

    /// The node's height as of its last evaluation (sources are 0).
    pub fn height(&self, n: NodeId) -> Option<u32> {
        Some(self.inner.borrow().nodes.get(n)?.height)
    }

    /// The scope that owns the node.
    pub fn scope_of(&self, n: NodeId) -> Option<ScopeId> {
        Some(self.inner.borrow().nodes.get(n)?.scope)
    }

    /// Dependencies captured by the last evaluation.
    pub fn sources(&self, n: NodeId) -> Vec<NodeId> {
        self.inner
            .borrow()
            .nodes
            .get(n)
            .map_or_else(Vec::new, |n| n.sources.iter().map(|e| e.node).collect())
    }

    /// Nodes that read this one in their last evaluation.
    pub fn observers(&self, n: NodeId) -> Vec<NodeId> {
        self.inner
            .borrow()
            .nodes
            .get(n)
            .map_or_else(Vec::new, |n| n.observers.iter().map(|e| e.node).collect())
    }

    /// Whether the node is evaluated eagerly (effects, promoted bindings,
    /// explicit roots, or every binding in eager mode).
    pub fn is_eager(&self, n: NodeId) -> bool {
        let i = self.inner.borrow();
        i.nodes
            .get(n)
            .is_some_and(|node| node.is_eager(i.options.eager))
    }

    /// Makes a binding an effect root (or demotes it). Polish effects and
    /// accessibility observers use this.
    pub fn set_eager(&self, n: NodeId, eager: bool) -> Result<(), ReactiveError> {
        {
            let mut g = self.inner.borrow_mut();
            let i = &mut *g;
            let all = i.options.eager;
            let node = i.nodes.get_mut(n).ok_or(ReactiveError::DeadNode(n))?;
            node.eager = eager;
            if node.state != NodeState::Clean
                && node.state != NodeState::Computing
                && node.is_eager(all)
            {
                enqueue(&mut i.queues, n, node);
            }
        }
        self.flush_if_idle();
        Ok(())
    }

    /// Attaches an opaque tag to a node. The graph never interprets it; the
    /// engine uses it to map a node back to its (object, property) when it
    /// prints a [`Diagnostic`].
    pub fn set_node_data(&self, n: NodeId, data: u64) -> Result<(), ReactiveError> {
        let mut i = self.inner.borrow_mut();
        i.nodes.get_mut(n).ok_or(ReactiveError::DeadNode(n))?.data = Some(data);
        Ok(())
    }

    /// The tag set by [`Graph::set_node_data`]; `None` if unset or the node
    /// is dead.
    pub fn node_data(&self, n: NodeId) -> Option<u64> {
        self.inner.borrow().nodes.get(n)?.data
    }

    /// Verifies the internal indices: every source entry has a mirror entry
    /// in the source's observer list pointing back at it, and the same for
    /// observers; the scope child lists are consistent in both directions.
    /// Costs O(graph); meant for tests and debug builds.
    pub fn check_invariants(&self) -> Result<(), String> {
        let i = self.inner.borrow();
        for (id, node) in &i.nodes {
            for (idx, e) in node.sources.iter().enumerate() {
                let src = i
                    .nodes
                    .get(e.node)
                    .ok_or_else(|| format!("{id:?}: source {:?} is dead", e.node))?;
                match src.observers.get(e.back as usize) {
                    Some(m) if m.node == id && m.back as usize == idx => {}
                    _ => {
                        return Err(format!(
                            "{id:?}: sources[{idx}] -> {:?}.observers[{}] does not point back",
                            e.node, e.back
                        ));
                    }
                }
            }
            for (idx, e) in node.observers.iter().enumerate() {
                let obs = i
                    .nodes
                    .get(e.node)
                    .ok_or_else(|| format!("{id:?}: observer {:?} is dead", e.node))?;
                match obs.sources.get(e.back as usize) {
                    Some(m) if m.node == id && m.back as usize == idx => {}
                    _ => {
                        return Err(format!(
                            "{id:?}: observers[{idx}] -> {:?}.sources[{}] does not point back",
                            e.node, e.back
                        ));
                    }
                }
            }
        }
        for (id, scope) in &i.scopes {
            let mut seen = 0usize;
            let mut prev = None;
            let mut cur = scope.first_child;
            while let Some(c) = cur {
                let child = i
                    .scopes
                    .get(c)
                    .ok_or_else(|| format!("{id:?}: child {c:?} is dead"))?;
                if child.parent != Some(id) || child.prev != prev {
                    return Err(format!("{id:?}: child {c:?} has wrong parent or prev link"));
                }
                seen += 1;
                if seen > i.scopes.len() {
                    return Err(format!("{id:?}: child list loops"));
                }
                prev = Some(c);
                cur = child.next;
            }
            if scope.last_child != prev {
                return Err(format!("{id:?}: last_child does not end the child list"));
            }
        }
        Ok(())
    }

    /// Live nodes.
    pub fn len(&self) -> usize {
        self.inner.borrow().nodes.len()
    }

    /// No live nodes.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Evaluation counters.
    pub fn stats(&self) -> Stats {
        self.inner.borrow().stats
    }

    // ----------------------------------------------------------- diagnostics

    /// Routes diagnostics to `handler` as they happen (outside any internal
    /// borrow, so the handler may use the graph). Without a handler they
    /// are retained for [`Graph::take_diagnostics`].
    pub fn set_diagnostic_handler(&self, handler: impl FnMut(&Diagnostic) + 'static) {
        self.inner.borrow_mut().handler = Some(Box::new(handler));
        self.dispatch_diagnostics();
    }

    /// Drains retained diagnostics.
    pub fn take_diagnostics(&self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.inner.borrow_mut().diagnostics)
    }

    fn dispatch_diagnostics(&self) {
        let (mut handler, diags) = {
            let mut i = self.inner.borrow_mut();
            if i.diagnostics.is_empty() || i.handler.is_none() {
                return;
            }
            (i.handler.take(), std::mem::take(&mut i.diagnostics))
        };
        if let Some(h) = handler.as_mut() {
            for d in &diags {
                h(d);
            }
        }
        let mut i = self.inner.borrow_mut();
        if i.handler.is_none() {
            i.handler = handler;
        }
    }

    // ------------------------------------------------------------ algorithm

    fn flush_if_idle(&self) {
        if self.inner.borrow().batch_depth == 0 {
            self.flush();
        }
    }

    /// Runs queued effect roots in (height, creation) order until none are
    /// left. Re-entrant: a write inside an effect flushes synchronously, as
    /// Qt's `onXChanged` handlers do.
    fn flush(&self) {
        {
            let mut i = self.inner.borrow_mut();
            if i.batch_depth > 0 || i.queues.last().is_none_or(BinaryHeap::is_empty) {
                return;
            }
            if i.flush_depth == 0 {
                i.flush_epoch += 1;
                i.stats.flushes += 1;
            }
            i.flush_depth += 1;
        }
        let _guard = FlushGuard(self);
        loop {
            // A statement, so the borrow ends before user code runs.
            let next = self.inner.borrow_mut().next_to_run();
            let Some(n) = next else {
                break;
            };
            let _level = {
                let mut i = self.inner.borrow_mut();
                (i.flush_depth >= SHARED_FLUSH_DEPTH).then(|| {
                    i.queues.push(Queue::new());
                    LevelGuard(self)
                })
            };
            self.update(n);
        }
        self.dispatch_diagnostics();
    }

    /// Brings `n` up to date: Check walks its sources (iteratively, so long
    /// chains do not grow the call stack) and recomputes only what changed.
    fn update(&self, n: NodeId) {
        let state = match self.inner.borrow().nodes.get(n) {
            Some(node) => node.state,
            None => return,
        };
        match state {
            // A node that is computing is not re-entered: the reader gets
            // the stale value. That alone is not a loop (the reader may be
            // unrelated, run by a nested flush). If it is one, the reader
            // becomes an observer and the node's new value comes back to
            // it in `finish`, where the loop is reported and broken.
            NodeState::Clean | NodeState::Computing => return,
            NodeState::Dirty => {
                self.recompute(n);
                return;
            }
            NodeState::Check => {}
        }
        enum Step {
            Pop,
            Recompute(NodeId),
            Push(NodeId),
            Next,
        }
        let mut stack = {
            let mut i = self.inner.borrow_mut();
            i.nodes[n].walking = true;
            let mut s = i.walk_pool.pop().unwrap_or_default();
            s.push((n, 0));
            s
        };
        while let Some(&(top, idx)) = stack.last() {
            let step = {
                let mut g = self.inner.borrow_mut();
                let i = &mut *g;
                match i.nodes.get(top).map(|t| t.state) {
                    None | Some(NodeState::Clean) | Some(NodeState::Computing) => Step::Pop,
                    Some(NodeState::Dirty) => Step::Recompute(top),
                    Some(NodeState::Check) => {
                        let node = &i.nodes[top];
                        if let Some(src) = node.sources.get(idx as usize).map(|e| e.node) {
                            if let Some(e) = stack.last_mut() {
                                e.1 += 1;
                            }
                            match i.nodes.get(src).map(|s| (s.state, s.walking)) {
                                // A source that is computing or already
                                // on a walk counts as unchanged for now.
                                // If it does change, its marks come back
                                // here, and a real cycle is reported when
                                // they return to the node that sent them.
                                None
                                | Some((NodeState::Clean, _))
                                | Some((NodeState::Check, true))
                                | Some((NodeState::Computing, _)) => Step::Next,
                                Some((NodeState::Check, false)) => {
                                    i.nodes[src].walking = true;
                                    Step::Push(src)
                                }
                                Some((NodeState::Dirty, _)) => Step::Recompute(src),
                            }
                        } else {
                            i.nodes[top].state = NodeState::Clean;
                            Step::Pop
                        }
                    }
                }
            };
            match step {
                Step::Pop => {
                    self.unwalk(top);
                    stack.pop();
                }
                Step::Recompute(x) => {
                    if x == top {
                        self.unwalk(top);
                        stack.pop();
                    }
                    self.recompute(x);
                }
                Step::Push(src) => stack.push((src, 0)),
                Step::Next => {}
            }
        }
        self.inner.borrow_mut().walk_pool.push(stack);
    }

    fn unwalk(&self, n: NodeId) {
        if let Some(node) = self.inner.borrow_mut().nodes.get_mut(n) {
            node.walking = false;
        }
    }

    fn recompute(&self, n: NodeId) {
        let (compute, gen_) = {
            let mut g = self.inner.borrow_mut();
            let i = &mut *g;
            let Some(node) = i.nodes.get_mut(n) else {
                return;
            };
            let compute = match &mut node.kind {
                Kind::Property(p) => match &mut p.top_mut().content {
                    Content::Binding(f) => f.take().map(Compute::Binding),
                    Content::Value(_) => None,
                },
                Kind::Effect(f) => f.take().map(Compute::Effect),
            };
            let Some(compute) = compute else {
                // A plain value has nothing to run.
                if node.state != NodeState::Computing {
                    node.state = NodeState::Clean;
                }
                return;
            };
            node.state = NodeState::Computing;
            node.pending = false;
            node.held = false;
            match compute {
                Compute::Binding(_) => i.stats.evaluations += 1,
                Compute::Effect(_) => i.stats.effect_runs += 1,
            }
            i.tick += 1;
            let reads = i.read_pool.pop().unwrap_or_default();
            i.frames.push(Frame {
                node: Some(n),
                stamp: i.tick,
                reads,
            });
            i.eval_stack.push(n);
            (compute, node.binding_gen)
        };
        let mut guard = ComputeGuard {
            graph: self,
            node: n,
            gen_,
            compute: Some(compute),
            value: None,
            ok: false,
        };
        match guard.compute.as_mut() {
            Some(Compute::Binding(f)) => guard.value = Some(f(self)),
            Some(Compute::Effect(f)) => f(self),
            None => {}
        }
        guard.ok = true;
        // `finish` runs here; a cycle it found is settled outside it.
        drop(guard);
        self.settle(n);
        self.run_after(n);
    }

    /// Runs `f` when the node that is computing now has its value: at
    /// once when none is. For what a binding's evaluation sets off but may
    /// not do itself, a write to its own property above all; the property
    /// then has the binding's value before the statement that changed the
    /// binding's input returns, and `f` has run too.
    pub fn after_evaluation(&self, f: impl FnOnce() + 'static) {
        let running = self.inner.borrow().eval_stack.last().copied();
        match running {
            Some(n) => self.inner.borrow_mut().after.push((n, Box::new(f))),
            None => f(),
        }
    }

    /// What waited for `n`, in the order it was put off. Whatever is left
    /// when nothing computes goes too (the node it waited for unwound).
    fn run_after(&self, n: NodeId) {
        let due: Vec<Box<dyn FnOnce()>> = {
            let mut g = self.inner.borrow_mut();
            let i = &mut *g;
            if i.after.is_empty() {
                return;
            }
            let all = i.eval_stack.is_empty();
            let (due, rest) = std::mem::take(&mut i.after)
                .into_iter()
                .partition(|(id, _)| all || *id == n);
            i.after = rest;
            due.into_iter().map(|(_, f)| f).collect()
        };
        for f in due {
            f();
        }
    }

    /// Breaks the dependency cycle `finish` found through `n`.
    ///
    /// Qt re-evaluates the bindings a change reaches before the write
    /// returns, and stops with its "Binding loop detected" warning when the
    /// change arrives back at the binding that started it. This does the
    /// same: the rest of the cycle is brought up to date once against `n`'s
    /// new value, and the mark that would make `n` run again is dropped.
    /// Without it the members would re-dirty each other on every read (or
    /// up to the re-run cap in eager mode) and never agree.
    ///
    /// The mark that came back only says the change may travel round. Qt
    /// warns when it does: when a member gives a new value and so the
    /// binding is asked to run again. A cycle whose members settle on the
    /// values they had (two views that follow each other's current index)
    /// is no loop, and is not reported.
    fn settle(&self, n: NodeId) {
        let (sources, cycle): (SmallVec<[NodeId; 4]>, Vec<NodeId>) = {
            let mut g = self.inner.borrow_mut();
            let i = &mut *g;
            let Some(node) = i.nodes.get_mut(n) else {
                return;
            };
            if !node.cyclic {
                return;
            }
            node.cyclic = false;
            node.settling = true;
            let sources = node.sources.iter().map(|e| e.node).collect();
            // Found now, while the members the change went through are
            // still marked.
            (sources, i.find_cycle(n))
        };
        let _guard = SettleGuard(self, n);
        // Every path from `n` back to itself ends in one of its sources,
        // so pulling those evaluates exactly the members of its cycles.
        for s in sources {
            self.update(s);
        }
        let mut g = self.inner.borrow_mut();
        let i = &mut *g;
        if let Some(node) = i.nodes.get_mut(n)
            && std::mem::take(&mut node.came_back)
        {
            i.emit(Diagnostic::BindingLoop { node: n, cycle });
        }
    }

    fn finish(
        &self,
        n: NodeId,
        gen_: u32,
        compute: Option<Compute<V>>,
        value: Option<V>,
        ok: bool,
    ) {
        let mut leftover = None;
        let mut leftover_value = None;
        {
            let mut g = self.inner.borrow_mut();
            let i = &mut *g;
            let frame = i.frames.pop();
            debug_assert!(frame.as_ref().is_some_and(|f| f.node == Some(n)));
            i.eval_stack.pop();
            let stamp = frame.as_ref().map_or(0, |f| f.stamp);
            let mut reads = frame.map(|f| f.reads).unwrap_or_default();
            let current = i.nodes.get(n).is_some_and(|node| node.binding_gen == gen_);
            if !current {
                // Disposed, or the binding was replaced while it ran: the
                // result is obsolete and the closure is not reinstalled.
                leftover = compute;
                leftover_value = value;
            } else {
                let node = &mut i.nodes[n];
                match (&mut node.kind, compute) {
                    (Kind::Property(p), Some(Compute::Binding(f))) => {
                        if let Content::Binding(slot) = &mut p.top_mut().content {
                            *slot = Some(f);
                        }
                    }
                    (Kind::Effect(slot), Some(Compute::Effect(f))) => *slot = Some(f),
                    (_, other) => leftover = other,
                }
                if ok {
                    i.diff_sources(n, &reads);
                    i.nodes[n].done_stamp = stamp;
                    let node = &mut i.nodes[n];
                    node.state = NodeState::Clean;
                    let self_read = std::mem::take(&mut node.self_read);
                    if let Some(v) = value {
                        if let Kind::Property(p) = &mut node.kind {
                            p.animated = false;
                        }
                        // Only the announcement of a binding's own result
                        // can close a cycle; marks sent for other reasons
                        // (writes, `notify`) have no origin.
                        i.wave_origin = Some(n);
                        let changed = i.set_value(n, v);
                        i.wave_origin = None;
                        // A self-read leaves no edge, so no mark returns;
                        // a change of what it read is its change coming
                        // back.
                        if changed && self_read {
                            i.nodes[n].cyclic = true;
                            i.nodes[n].came_back = true;
                        }
                    }
                    if i.nodes[n].pending {
                        // It runs again anyway; that run settles the cycle
                        // if its result still travels round.
                        i.nodes[n].pending = false;
                        i.nodes[n].cyclic = false;
                        i.redirty(n);
                    }
                } else {
                    // Panicked: keep the old edges so the node still hears
                    // about changes. A binding retries on its next read; an
                    // effect waits for its next change rather than looping.
                    let node = &mut i.nodes[n];
                    node.pending = false;
                    node.self_read = false;
                    if matches!(node.kind, Kind::Effect(_)) {
                        node.state = NodeState::Clean;
                    } else {
                        node.state = NodeState::Dirty;
                        i.mark_observers(n, NodeState::Check);
                    }
                }
            }
            reads.clear();
            i.read_pool.push(reads);
        }
        drop(leftover);
        drop(leftover_value);
    }
}

/// Mark stamp meaning "the sender is gone or unknown": a computing receiver
/// must assume it read stale data.
const ALWAYS_STALE: u64 = u64::MAX;

/// Removes `src.observers[at]` and repairs the back-index of the entry that
/// the swap moved into its place. Returns the removed entry. The mirror
/// entry in the observer's `sources` is the caller's to remove.
fn drop_observer_slot<V>(
    nodes: &mut SlotMap<NodeId, Node<V>>,
    src: NodeId,
    at: u32,
) -> Option<Edge> {
    let list = &mut nodes.get_mut(src)?.observers;
    if at as usize >= list.len() {
        debug_assert!(false, "observer slot out of range");
        return None;
    }
    let removed = list.swap_remove(at as usize);
    let moved = list.get(at as usize).copied();
    if let Some(m) = moved
        && let Some(entry) = nodes
            .get_mut(m.node)
            .and_then(|o| o.sources.get_mut(m.back as usize))
    {
        entry.back = at;
    }
    Some(removed)
}

/// The mirror of [`drop_observer_slot`] for `obs.sources[at]`.
fn drop_source_slot<V>(nodes: &mut SlotMap<NodeId, Node<V>>, obs: NodeId, at: u32) -> Option<Edge> {
    let list = &mut nodes.get_mut(obs)?.sources;
    if at as usize >= list.len() {
        debug_assert!(false, "source slot out of range");
        return None;
    }
    let removed = list.swap_remove(at as usize);
    let moved = list.get(at as usize).copied();
    if let Some(m) = moved
        && let Some(entry) = nodes
            .get_mut(m.node)
            .and_then(|s| s.observers.get_mut(m.back as usize))
    {
        entry.back = at;
    }
    Some(removed)
}

type Queue = BinaryHeap<Reverse<(u32, u64, NodeId)>>;

/// How deep flushes nest before an effect's writes flush only what they
/// queued (`Inner::queues`).
const SHARED_FLUSH_DEPTH: u32 = 8;

/// Queues `n` on the innermost flush level. A node already waiting on an
/// outer level moves in; its old entry is skipped when it comes up, as it
/// is no longer queued there.
fn enqueue<V>(queues: &mut [Queue], n: NodeId, node: &mut Node<V>) {
    let top = queues.len().saturating_sub(1);
    if node.deferred || (node.queued && node.queued_level as usize >= top) {
        return;
    }
    node.queued = true;
    node.queued_level = top as u32;
    queues[top].push(Reverse((node.height, node.seq, n)));
}

impl<V: Clone + 'static> Inner<V> {
    fn insert_node(
        &mut self,
        scope: ScopeId,
        kind: Kind<V>,
        value: Option<V>,
        state: NodeState,
    ) -> Result<NodeId, ReactiveError> {
        if !self.scopes.contains_key(scope) {
            return Err(ReactiveError::DeadScope(scope));
        }
        self.seq += 1;
        let n = self.nodes.insert(Node {
            kind,
            value,
            state,
            pending: false,
            eager: false,
            queued: false,
            queued_level: 0,
            deferred: false,
            walking: false,
            self_read: false,
            cyclic: false,
            settling: false,
            came_back: false,
            held: false,
            height: 0,
            seq: self.seq,
            scope,
            sources: Edges::new(),
            observers: Edges::new(),
            mark: 0,
            scratch: 0,
            data: None,
            read_stamp: 0,
            done_stamp: 0,
            runs: 0,
            run_epoch: 0,
            binding_gen: 0,
            asked_if_set: false,
        });
        self.scopes[scope].nodes.push(n);
        Ok(n)
    }

    /// Pops the next runnable root, parking frozen ones and enforcing the
    /// per-flush re-run cap.
    fn next_to_run(&mut self) -> Option<NodeId> {
        let level = self.queues.len() - 1;
        while let Some(Reverse((_, _, n))) = self.queues[level].pop() {
            let Some(node) = self.nodes.get_mut(n) else {
                continue;
            };
            if !node.queued || node.queued_level as usize != level {
                continue;
            }
            node.queued = false;
            match node.state {
                NodeState::Clean => continue,
                NodeState::Computing => {
                    node.pending = true;
                    continue;
                }
                NodeState::Check | NodeState::Dirty => {}
            }
            if let Some(scope) = self.scopes.get_mut(node.scope)
                && scope.frozen
            {
                node.deferred = true;
                scope.deferred.push(n);
                continue;
            }
            if node.run_epoch != self.flush_epoch {
                node.run_epoch = self.flush_epoch;
                node.runs = 0;
            }
            node.runs += 1;
            let cap = self.options.max_reruns_per_flush;
            if node.runs > cap {
                // Leave it clean so later changes can still wake it.
                node.state = NodeState::Clean;
                if node.runs == cap + 1 {
                    self.emit(Diagnostic::RerunLimit { node: n, runs: cap });
                }
                continue;
            }
            return Some(n);
        }
        None
    }

    fn emit(&mut self, d: Diagnostic) {
        if self.handler.is_none() && self.diagnostics.len() >= self.options.max_retained_diagnostics
        {
            self.dropped_diagnostics += 1;
            return;
        }
        self.diagnostics.push(d);
    }

    /// The cycle through `n`: `n`, then one chain of sources that its
    /// change just dirtied and that leads back to it.
    fn find_cycle(&mut self, n: NodeId) -> Vec<NodeId> {
        self.tick += 1;
        let seen = self.tick;
        let mut cycle = vec![n];
        // (node, next source to try); the path is the stack.
        let mut stack: Vec<(NodeId, usize)> = vec![(n, 0)];
        self.nodes[n].mark = seen;
        'search: while let Some(&(top, idx)) = stack.last() {
            let Some(src) = self.nodes[top].sources.get(idx).map(|e| e.node) else {
                stack.pop();
                continue;
            };
            if let Some(e) = stack.last_mut() {
                e.1 += 1;
            }
            let Some(node) = self.nodes.get_mut(src) else {
                continue;
            };
            // The change only went through nodes it left non-clean.
            if node.mark == seen || node.state == NodeState::Clean {
                continue;
            }
            node.mark = seen;
            stack.push((src, 0));
            if node.sources.iter().any(|e| e.node == n) {
                cycle = stack.iter().map(|(id, _)| *id).collect();
                break 'search;
            }
        }
        cycle
    }

    /// Stores `v` if it differs and marks direct observers dirty. Returns
    /// whether it differed.
    fn set_value(&mut self, n: NodeId, v: V) -> bool {
        let eq = self.options.equals;
        let node = &mut self.nodes[n];
        if node.value.as_ref().is_some_and(|old| eq(old, &v)) {
            return false;
        }
        node.value = Some(v);
        self.stats.propagations += 1;
        self.mark_observers(n, NodeState::Dirty);
        true
    }

    /// Whether nothing has given the property a value: what
    /// [`Graph::provenance`] calls [`Provenance::Default`].
    fn unset(&self, n: NodeId) -> bool {
        match self.nodes.get(n).map(|node| &node.kind) {
            Some(Kind::Property(p)) => !p.animated && p.top().provenance == Provenance::Default,
            _ => false,
        }
    }

    /// After a change of the active layer: tells the observers of a
    /// property that was asked about ([`Graph::provenance_tracked`]) that
    /// it is now set where it was not, or the other way round. A change of
    /// the value has told them already; telling twice costs nothing.
    fn tell_if_set(&mut self, n: NodeId, was_unset: bool) {
        if self.nodes.get(n).is_some_and(|node| node.asked_if_set) && self.unset(n) != was_unset {
            self.mark_observers(n, NodeState::Dirty);
        }
    }

    /// The active layer is now a plain value: drop dependency edges.
    fn make_plain(&mut self, n: NodeId) {
        // Back-indices in the (detached) list are only touched through the
        // mirror entries, so draining it in any order is fine.
        let sources = std::mem::take(&mut self.nodes[n].sources);
        for e in &sources {
            drop_observer_slot(&mut self.nodes, e.node, e.back);
        }
        let node = &mut self.nodes[n];
        node.binding_gen = node.binding_gen.wrapping_add(1);
        node.held = false;
        node.height = 0;
        if node.state != NodeState::Computing {
            node.state = NodeState::Clean;
        } else {
            node.pending = false;
        }
    }

    /// The active layer is now a (new) binding: re-run it, observers check.
    fn make_bound_dirty(&mut self, n: NodeId) {
        let all = self.options.eager;
        let node = &mut self.nodes[n];
        node.binding_gen = node.binding_gen.wrapping_add(1);
        node.held = false;
        node.state = NodeState::Dirty;
        node.pending = false;
        if node.is_eager(all) {
            enqueue(&mut self.queues, n, node);
        }
        self.mark_observers(n, NodeState::Check);
    }

    /// The node was marked while it ran: it is dirty again.
    fn redirty(&mut self, n: NodeId) {
        let all = self.options.eager;
        let node = &mut self.nodes[n];
        node.state = NodeState::Dirty;
        if node.is_eager(all) {
            enqueue(&mut self.queues, n, node);
        }
        self.mark_observers(n, NodeState::Check);
    }

    fn is_current(&self, r: Reader) -> bool {
        let Some(node) = self.nodes.get(r.node) else {
            return false;
        };
        node.done_stamp == r.run
            || self
                .frames
                .iter()
                .rev()
                .find(|f| f.node == Some(r.node))
                .is_some_and(|f| f.stamp == r.run)
    }

    /// Records a read of `n` in the innermost tracking frame.
    fn record_read(&mut self, n: NodeId) {
        let Inner {
            nodes,
            frames,
            stats,
            ..
        } = self;
        let node = &mut nodes[n];
        let Some(frame) = frames.last_mut() else {
            return;
        };
        match frame.node {
            None => {}
            Some(owner) if owner == n => node.self_read = true,
            Some(_) if node.read_stamp != frame.stamp => {
                node.read_stamp = frame.stamp;
                frame.reads.push(n);
                stats.reads += 1;
            }
            Some(_) => {}
        }
    }

    fn mark_observers(&mut self, n: NodeId, first: NodeState) {
        let mut stack = std::mem::take(&mut self.mark_stack);
        if let Some(node) = self.nodes.get(n) {
            let stamp = node.read_stamp;
            stack.extend(node.observers.iter().map(|o| (o.node, first, stamp)));
        }
        self.mark_stack = stack;
        self.propagate_marks();
    }

    /// Dirty for direct observers, Check for everything downstream. A node
    /// that was already non-clean has already passed Check on.
    fn propagate_marks(&mut self) {
        let all = self.options.eager;
        let mut stack = std::mem::take(&mut self.mark_stack);
        while let Some((m, st, from_stamp)) = stack.pop() {
            let Some(node) = self.nodes.get_mut(m) else {
                continue;
            };
            if self.wave_origin == Some(m) {
                // The node's own change came back: a dependency cycle.
                // `settle` reports it and brings the members up to date.
                node.cyclic = true;
                continue;
            }
            if node.settling {
                // Its cycle is settling against the value it just
                // produced. Running it again would be the loop; a source
                // that changed says the change came back.
                if st == NodeState::Dirty {
                    node.came_back = true;
                }
                continue;
            }
            if node.state == NodeState::Computing {
                // Only news if the running node already read the sender in
                // this run; otherwise it is about to read the fresh value.
                let read = from_stamp == ALWAYS_STALE
                    || self
                        .frames
                        .iter()
                        .rev()
                        .find(|f| f.node == Some(m))
                        .is_some_and(|f| f.stamp == from_stamp);
                if read {
                    node.pending = true;
                }
                continue;
            }
            if node.state.rank() >= st.rank() {
                // Already waiting, maybe on an outer flush level: this
                // change is news to it, so it runs with this change's flush.
                if node.is_eager(all) {
                    enqueue(&mut self.queues, m, node);
                }
                continue;
            }
            let was_clean = node.state == NodeState::Clean;
            node.state = st;
            if node.is_eager(all) {
                enqueue(&mut self.queues, m, node);
            }
            if was_clean {
                let stamp = node.read_stamp;
                stack.extend(
                    node.observers
                        .iter()
                        .map(|o| (o.node, NodeState::Check, stamp)),
                );
            }
        }
        self.mark_stack = stack;
    }

    /// Replaces `n`'s sources with this run's reads, touching only edges
    /// that changed, and recomputes its height.
    fn diff_sources(&mut self, n: NodeId, reads: &[NodeId]) {
        // Most runs read what the last one did, in the same order: an
        // effect that follows a few properties of an item every frame.
        // Then every edge stays where it is, and only the height may move.
        if let Some(height) = self.same_sources(n, reads) {
            self.nodes[n].height = height;
            return;
        }
        self.stats.source_rebuilds += 1;
        self.tick += 1;
        let old_tag = self.tick;
        self.tick += 1;
        let new_tag = self.tick;
        // `n.sources` is detached while it is rebuilt, so every back-index
        // that points into it is fixed by hand below. Entries that move in
        // another node's list never belong to `n`: an edge is unique per
        // pair, and the only entry of `n` that moves is the one removed.
        let old = std::mem::take(&mut self.nodes[n].sources);
        for e in &old {
            if let Some(src) = self.nodes.get_mut(e.node) {
                src.mark = old_tag;
                src.scratch = e.back;
            }
        }
        let mut fresh = Edges::new();
        let mut height = 0;
        for &r in reads {
            if r == n {
                continue;
            }
            let Some(src) = self.nodes.get_mut(r) else {
                continue;
            };
            if src.mark == new_tag {
                continue;
            }
            let slot = if src.mark == old_tag {
                // Kept: only the position inside `n.sources` changes.
                let slot = src.scratch;
                src.observers[slot as usize].back = fresh.len() as u32;
                slot
            } else {
                src.observers.push(Edge {
                    node: n,
                    back: fresh.len() as u32,
                });
                src.observers.len() as u32 - 1
            };
            src.mark = new_tag;
            height = height.max(src.height + 1);
            fresh.push(Edge {
                node: r,
                back: slot,
            });
        }
        for e in &old {
            let stale = self
                .nodes
                .get(e.node)
                .is_some_and(|src| src.mark == old_tag);
            if stale {
                let slot = self.nodes[e.node].scratch;
                drop_observer_slot(&mut self.nodes, e.node, slot);
            }
        }
        let node = &mut self.nodes[n];
        node.sources = fresh;
        node.height = height.max(1);
    }

    /// `n`'s height if `reads` are its sources as they stand, in order and
    /// all alive; `None` if they have to be rebuilt.
    fn same_sources(&self, n: NodeId, reads: &[NodeId]) -> Option<u32> {
        let old = &self.nodes[n].sources;
        if old.len() != reads.len() {
            return None;
        }
        let mut height = 0;
        for (e, &r) in old.iter().zip(reads) {
            if e.node != r {
                return None;
            }
            height = height.max(self.nodes.get(r)?.height + 1);
        }
        Some(height.max(1))
    }

    /// Recomputes the effective frozen state of `scope` and its subtree
    /// from the parent's and each scope's own flag; parked effects of
    /// scopes that thawed are queued.
    fn refreeze(&mut self, scope: ScopeId) {
        let Some(s) = self.scopes.get(scope) else {
            return;
        };
        let parent_frozen = s
            .parent
            .and_then(|p| self.scopes.get(p))
            .is_some_and(|p| p.frozen);
        let mut thawed = Vec::new();
        let mut stack = vec![(scope, parent_frozen)];
        while let Some((sid, parent_frozen)) = stack.pop() {
            let Some(s) = self.scopes.get_mut(sid) else {
                continue;
            };
            let eff = parent_frozen || s.frozen_self;
            if eff == s.frozen && sid != scope {
                continue;
            }
            let was = s.frozen;
            s.frozen = eff;
            if was && !eff {
                thawed.append(&mut s.deferred);
            }
            let mut child = s.first_child;
            while let Some(c) = child {
                stack.push((c, eff));
                child = self.scopes.get(c).and_then(|c| c.next);
            }
        }
        let all = self.options.eager;
        for n in thawed {
            if let Some(node) = self.nodes.get_mut(n) {
                node.deferred = false;
                if node.state != NodeState::Clean && node.is_eager(all) {
                    enqueue(&mut self.queues, n, node);
                }
            }
        }
    }

    /// Unlinks a removed scope from its parent's child list.
    fn unlink_scope(&mut self, s: &Scope) {
        self.unlink_links(s.parent, s.prev, s.next);
    }

    /// Unlinks the scope with these links from its parent's child list.
    fn unlink_links(
        &mut self,
        parent: Option<ScopeId>,
        prev: Option<ScopeId>,
        next: Option<ScopeId>,
    ) {
        if let Some(p) = prev.and_then(|p| self.scopes.get_mut(p)) {
            p.next = next;
        } else if let Some(p) = parent.and_then(|p| self.scopes.get_mut(p)) {
            p.first_child = next;
        }
        if let Some(x) = next.and_then(|x| self.scopes.get_mut(x)) {
            x.prev = prev;
        } else if let Some(p) = parent.and_then(|p| self.scopes.get_mut(p)) {
            p.last_child = prev;
        }
    }
}
