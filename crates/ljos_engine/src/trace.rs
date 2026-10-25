//! Tracing an object: what it is told, as Erlang's trace writes down the
//! messages a process gets (ARCHITECTURE.md §3.4).
//!
//! An object is told things by its signals and by its properties
//! changing. A traced object's change signals are connected as a handler
//! of `xChanged` would be, so a change is told when the graph says the
//! value changed, in the order a QML handler would see it. Its other
//! signals are told where they are emitted, before any handler runs, so
//! that what a handler emits in turn comes after them; a connection would
//! run after the handlers connected before it, and would make a signal
//! that counts as taken once somebody hears it (`Keys`) taken. While
//! nothing is traced an emission pays one look at a flag.
//!
//! A leaf of a group (`border.color`) has no change signal of its own; it
//! is watched in the graph instead. The group is a field of the object, so
//! watching its leaves makes nothing that was not there.
//!
//! A trace can also follow the object's bindings: each time one runs
//! again it is told what the binding read that changed, how long it took
//! and whether it gave the value it had. The graph does not keep why a
//! node is dirty, so the trace keeps what each binding read, with the
//! values, when it last ran; what reads differ from that is the cause. A
//! read the graph gave a node only when it changed (`FieldReads`) is new
//! among the reads, which makes it a cause too. While no trace follows
//! bindings a binding's run pays one look at a flag.
//!
//! A binding written on an alias runs as part of the binding of what the
//! alias leads to, so it is the target's node that runs. It is told to a
//! trace of the object it was written on all the same, by the alias's
//! name, and an alias of one member is told changed with that member's
//! value, not the whole property's.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use ljos_reactive::NodeId;
use rustc_hash::FxHashMap;

use crate::engine::{BindingFn, Engine};
use crate::error::EngineError;
use crate::ids::{ConnectionId, ObjectId, PropIndex, SignalIndex};
use crate::meta::{PropertyAccess, PropertyFlags};
use crate::value::Value;

/// What a traced object was told.
#[derive(Clone, Copy, Debug)]
pub enum Traced<'a> {
    /// A signal other than a change signal was emitted, with these
    /// arguments.
    Signal {
        signal: SignalIndex,
        args: &'a [Value],
    },
    /// A property changed to `value`.
    Change { prop: PropIndex, value: &'a Value },
    /// A binding of `prop` ran again, for `us` microseconds. `cause` is
    /// what it read that changed since it last ran, as far as the trace
    /// saw; `same` says it gave the value it had, which is work for
    /// nothing.
    Binding {
        prop: PropIndex,
        cause: &'a [(ObjectId, PropIndex)],
        us: u64,
        same: bool,
    },
    /// The object is being destroyed. It is traced no longer.
    Destroyed,
}

/// The most reads of one binding a trace keeps to compare the next run
/// with: a binding that reads more is told the causes among these.
const MAX_READS: usize = 32;

/// Told what happens to the objects it traces. It runs where the signal is
/// emitted or the change is told, untracked; it must not write.
pub type TraceSink = Rc<dyn Fn(&Engine, ObjectId, Traced<'_>)>;

/// What each binding read when it last ran, with the values then.
type Reads = FxHashMap<PropIndex, Vec<(NodeId, Value)>>;

struct Trace {
    sink: TraceSink,
    conns: Vec<ConnectionId>,
    watches: Vec<NodeId>,
    /// `Some` while the trace follows the object's bindings.
    reads: Option<Reads>,
}

#[derive(Default)]
pub(crate) struct Traces {
    /// Whether any object is traced: the one look an emission pays.
    on: Cell<bool>,
    /// Whether any trace follows bindings: the one look a binding's run
    /// pays.
    bindings: Cell<bool>,
    by_object: RefCell<FxHashMap<ObjectId, Trace>>,
}

/// A run of a followed binding, from before it until after it.
pub(crate) struct BindingRun {
    obj: ObjectId,
    prop: PropIndex,
    node: NodeId,
    cause: Vec<(ObjectId, PropIndex)>,
    started: Instant,
}

impl Engine {
    /// Tells `sink` from now on what `obj` is told: its signals, its
    /// properties changing and its end. Tracing it again replaces the
    /// sink.
    pub fn trace(&self, obj: ObjectId, sink: TraceSink) -> Result<(), EngineError> {
        self.untrace(obj);
        let (changes, leaves): (Vec<(SignalIndex, PropIndex)>, Vec<PropIndex>) = {
            let store = self.inner.store.borrow();
            let h = store
                .objects
                .get(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            let leaves = h
                .meta
                .properties
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    matches!(p.access, PropertyAccess::Group { .. })
                        && !p.flags.contains(PropertyFlags::GROUP)
                })
                .map(|(i, _)| PropIndex(i as u32))
                .filter(|p| !h.makes_on_read(*p))
                .collect();
            let changes = h
                .meta
                .signals
                .iter()
                .enumerate()
                .filter_map(|(i, s)| Some((SignalIndex(i as u32), s.notify_of?)))
                .filter(|(_, p)| {
                    // A group is an object of its own, and what is made on
                    // its first read must not be made by looking.
                    h.meta
                        .property(*p)
                        .is_some_and(|p| !p.flags.contains(PropertyFlags::GROUP))
                        && !h.makes_on_read(*p)
                })
                .collect();
            (changes, leaves)
        };
        let mut conns = Vec::with_capacity(changes.len() + 1);
        for (signal, prop) in changes {
            let sink = sink.clone();
            // An alias of one member changes with the whole property and
            // is told it: what it changed to is the member.
            let member = self.alias_member(obj, prop).is_some();
            let told = self.connect(obj, signal, move |engine, args| {
                if member {
                    let value = engine.untrack(|| engine.read(obj, prop));
                    sink(
                        engine,
                        obj,
                        Traced::Change {
                            prop,
                            value: &value,
                        },
                    );
                    return;
                }
                let value = args.first().unwrap_or(&Value::Undefined);
                sink(engine, obj, Traced::Change { prop, value });
            });
            // A property the graph cannot follow is left out, not the
            // whole trace.
            if let Ok(c) = told {
                conns.push(c);
            }
        }
        let mut watches = Vec::with_capacity(leaves.len());
        for prop in leaves {
            let sink = sink.clone();
            let told = self.watch(obj, prop, move |engine, value| {
                sink(engine, obj, Traced::Change { prop, value });
            });
            if let Ok(n) = told {
                watches.push(n);
            }
        }
        let end = sink.clone();
        conns.push(self.connect(obj, SignalIndex::DESTROYED, move |engine, _| {
            engine.inner.traces.forget(obj);
            end(engine, obj, Traced::Destroyed);
        })?);
        let traces = &self.inner.traces;
        traces.by_object.borrow_mut().insert(
            obj,
            Trace {
                sink,
                conns,
                watches,
                reads: None,
            },
        );
        traces.on.set(true);
        Ok(())
    }

    /// Whether the trace of `obj` follows its bindings from now on: each
    /// time one runs again the sink is told ([`Traced::Binding`]). False if
    /// `obj` is not traced. Tracing it again stops it.
    pub fn trace_bindings(&self, obj: ObjectId, follow: bool) -> bool {
        let traces = &self.inner.traces;
        if !traces.by_object.borrow().contains_key(&obj) {
            return false;
        }
        let reads = follow.then(|| {
            // What each binding read is kept now, so that its first run
            // after this has something to be compared with.
            let (mut nodes, aliases): (Vec<(PropIndex, NodeId)>, Vec<PropIndex>) = {
                let store = self.inner.store.borrow();
                let Some(h) = store.objects.get(obj) else {
                    return FxHashMap::default();
                };
                let aliases = (0..h.meta.properties.len())
                    .map(|i| PropIndex(i as u32))
                    .filter(|p| h.meta.property(*p).is_some_and(|m| m.is_alias()))
                    .collect();
                let nodes = (0..h.meta.properties.len())
                    .filter_map(|i| {
                        let p = PropIndex(i as u32);
                        Some((p, h.node(p)?))
                    })
                    .collect();
                (nodes, aliases)
            };
            // A binding written on an alias is the node of what it leads to.
            for p in aliases {
                let (to, tp) = self.redirect(obj, p);
                let n = self
                    .inner
                    .store
                    .borrow()
                    .objects
                    .get(to)
                    .and_then(|h| h.node(tp));
                if let Some(n) = n {
                    nodes.push((p, n));
                }
            }
            nodes
                .into_iter()
                .filter(|(_, n)| self.inner.graph.is_bound(*n))
                .map(|(p, n)| (p, self.reads_now(n)))
                .collect()
        });
        if let Some(t) = traces.by_object.borrow_mut().get_mut(&obj) {
            t.reads = reads;
        }
        traces.recount();
        true
    }

    /// Whether the trace of `obj` follows its bindings.
    pub fn traces_bindings(&self, obj: ObjectId) -> bool {
        self.inner
            .traces
            .by_object
            .borrow()
            .get(&obj)
            .is_some_and(|t| t.reads.is_some())
    }

    /// What `n` read when it last ran, with the values they have now.
    pub(crate) fn reads_now(&self, n: NodeId) -> Vec<(NodeId, Value)> {
        let g = &self.inner.graph;
        g.sources(n)
            .into_iter()
            .take(MAX_READS)
            .filter_map(|s| Some((s, g.peek(s)?)))
            .collect()
    }

    /// The start of a run of the binding of `prop`, if a trace follows
    /// the bindings of `obj`: what it read that changed since its last run.
    pub(crate) fn binding_runs(&self, obj: ObjectId, prop: PropIndex) -> Option<BindingRun> {
        let node = self.inner.store.borrow().objects.get(obj)?.node(prop)?;
        self.binding_runs_at(obj, prop, node)
    }

    /// [`Engine::binding_runs`] for a binding whose node is `node`, which
    /// for one written on an alias is the node of the property it leads to.
    fn binding_runs_at(&self, obj: ObjectId, prop: PropIndex, node: NodeId) -> Option<BindingRun> {
        let kept = {
            let by_object = self.inner.traces.by_object.borrow();
            let reads = by_object.get(&obj)?.reads.as_ref()?;
            reads.get(&prop).cloned().unwrap_or_default()
        };
        let g = &self.inner.graph;
        let changed: Vec<NodeId> = g
            .sources(node)
            .into_iter()
            .take(MAX_READS)
            .filter(|s| match kept.iter().find(|(k, _)| k == s) {
                Some((_, was)) => g.peek(*s).is_some_and(|now| now != *was),
                // The graph gives a read a node when what it read changed.
                None => true,
            })
            .collect();
        let cause = {
            let store = self.inner.store.borrow();
            changed
                .into_iter()
                .filter_map(|s| store.node_keys.get(s).map(|k| (k.object, k.prop)))
                .collect()
        };
        Some(BindingRun {
            obj,
            prop,
            node,
            cause,
            started: Instant::now(),
        })
    }

    /// A binding written on the alias `prop` of `obj`, which runs as part of
    /// the binding of `to`, the property the alias leads to: a trace of
    /// `obj` that follows its bindings is told of each run by the alias's
    /// name, with what the binding of `to` read that changed. Whether it
    /// gave what it had is the alias's value, not the whole property's,
    /// when the alias is of one member.
    pub(crate) fn traced_as_alias(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        (to, tp): (ObjectId, PropIndex),
        mut f: BindingFn,
    ) -> BindingFn {
        let mut last = Value::Undefined;
        Box::new(move |engine| {
            let followed = if engine.inner.traces.follows_bindings() {
                engine.untrack(|| {
                    let node = engine.inner.store.borrow().objects.get(to)?.node(tp)?;
                    engine.binding_runs_at(obj, prop, node)
                })
            } else {
                None
            };
            let v = f(engine)?;
            if let Some(run) = followed {
                engine.binding_ran(run, &last, &v);
            }
            last = v.clone();
            Some(v)
        })
    }

    /// The end of a run [`Engine::binding_runs`] began: tells the trace,
    /// and keeps what the binding read once the graph has its reads.
    pub(crate) fn binding_ran(&self, run: BindingRun, before: &Value, gave: &Value) {
        let us = run.started.elapsed().as_micros() as u64;
        let sink = self
            .inner
            .traces
            .by_object
            .borrow()
            .get(&run.obj)
            .map(|t| t.sink.clone());
        if let Some(sink) = sink {
            let what = Traced::Binding {
                prop: run.prop,
                cause: &run.cause,
                us,
                same: before == gave,
            };
            self.untrack(|| sink(self, run.obj, what));
        }
        let weak = Rc::downgrade(&self.inner);
        let (obj, prop, node) = (run.obj, run.prop, run.node);
        self.inner.graph.after_evaluation(move || {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let engine = Engine::from_inner(inner);
            let reads = engine.reads_now(node);
            if let Some(Some(kept)) = engine
                .inner
                .traces
                .by_object
                .borrow_mut()
                .get_mut(&obj)
                .map(|t| t.reads.as_mut())
            {
                kept.insert(prop, reads);
            }
        });
    }

    /// Stops tracing `obj`. False if it was not traced.
    pub fn untrace(&self, obj: ObjectId) -> bool {
        let Some(trace) = self.inner.traces.forget(obj) else {
            return false;
        };
        for c in trace.conns {
            self.disconnect(c);
        }
        for n in trace.watches {
            // Gone already if the object's scope went first.
            let _ = self.inner.graph.dispose(n);
        }
        true
    }

    /// The objects traced, in no order.
    pub fn traced(&self) -> Vec<ObjectId> {
        self.inner
            .traces
            .by_object
            .borrow()
            .keys()
            .copied()
            .collect()
    }

    /// Tells the trace of `obj`, if it has one, that `signal` was emitted.
    #[inline]
    pub(crate) fn tell_trace(&self, obj: ObjectId, signal: SignalIndex, args: &[Value]) {
        if !self.inner.traces.on.get() {
            return;
        }
        let sink = self
            .inner
            .traces
            .by_object
            .borrow()
            .get(&obj)
            .map(|t| t.sink.clone());
        if let Some(sink) = sink {
            self.untrack(|| sink(self, obj, Traced::Signal { signal, args }));
        }
    }
}

impl Traces {
    fn forget(&self, obj: ObjectId) -> Option<Trace> {
        let trace = self.by_object.borrow_mut().remove(&obj);
        self.recount();
        trace
    }

    fn recount(&self) {
        let by_object = self.by_object.borrow();
        self.on.set(!by_object.is_empty());
        self.bindings
            .set(by_object.values().any(|t| t.reads.is_some()));
    }

    /// Whether any trace follows bindings.
    #[inline]
    pub(crate) fn follows_bindings(&self) -> bool {
        self.bindings.get()
    }
}
