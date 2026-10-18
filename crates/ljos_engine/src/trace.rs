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

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_reactive::NodeId;
use rustc_hash::FxHashMap;

use crate::engine::Engine;
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
    /// The object is being destroyed. It is traced no longer.
    Destroyed,
}

/// Told what happens to the objects it traces. It runs where the signal is
/// emitted or the change is told, untracked; it must not write.
pub type TraceSink = Rc<dyn Fn(&Engine, ObjectId, Traced<'_>)>;

struct Trace {
    sink: TraceSink,
    conns: Vec<ConnectionId>,
    watches: Vec<NodeId>,
}

#[derive(Default)]
pub(crate) struct Traces {
    /// Whether any object is traced: the one look an emission pays.
    on: Cell<bool>,
    by_object: RefCell<FxHashMap<ObjectId, Trace>>,
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
            let told = self.connect(obj, signal, move |engine, args| {
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
            },
        );
        traces.on.set(true);
        Ok(())
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
        let mut by_object = self.by_object.borrow_mut();
        let trace = by_object.remove(&obj);
        self.on.set(!by_object.is_empty());
        trace
    }
}
