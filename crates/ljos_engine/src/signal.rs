//! Signals and connections (ARCHITECTURE.md §6.2).
//!
//! Handlers run synchronously, inside [`Engine::emit`], in connection
//! order, untracked. A property's change signal (`widthChanged`) is not
//! emitted by hand: connecting to it subscribes to the property's reactive
//! node ([`ljos_reactive::Graph::watch`]), so it fires exactly when the
//! graph says the value changed, after the write that changed it and in
//! the graph's flush order.

use std::rc::Rc;

use ljos_reactive::NodeId;
use slotmap::{SecondaryMap, SlotMap};
use smallvec::SmallVec;

use crate::engine::{Engine, Tracking};
use crate::error::EngineError;
use crate::ids::{ConnectionId, ObjectId, SignalIndex};
use crate::meta::MetaObject;
use crate::value::Value;

/// A signal handler: the engine and the signal's arguments.
pub type Handler = Rc<dyn Fn(&Engine, &[Value])>;

pub(crate) struct Connection {
    pub(crate) sender: ObjectId,
    pub(crate) signal: SignalIndex,
    pub(crate) receiver: Option<ObjectId>,
    pub(crate) handler: Handler,
    /// The graph subscription behind a change-signal connection.
    pub(crate) watch: Option<NodeId>,
}

#[derive(Default)]
pub(crate) struct Signals {
    conns: SlotMap<ConnectionId, Connection>,
    /// Connections per object, as sender or receiver.
    by_object: SecondaryMap<ObjectId, SmallVec<[ConnectionId; 2]>>,
}

impl Signals {
    fn link(&mut self, obj: ObjectId, id: ConnectionId) {
        match self.by_object.entry(obj) {
            Some(slotmap::secondary::Entry::Occupied(mut e)) => e.get_mut().push(id),
            Some(slotmap::secondary::Entry::Vacant(e)) => {
                e.insert(SmallVec::from_elem(id, 1));
            }
            None => {}
        }
    }

    fn unlink(&mut self, obj: ObjectId, id: ConnectionId) {
        if let Some(v) = self.by_object.get_mut(obj) {
            v.retain(|c| *c != id);
        }
    }

    fn insert(&mut self, c: Connection) -> ConnectionId {
        let (sender, receiver) = (c.sender, c.receiver);
        let id = self.conns.insert(c);
        self.link(sender, id);
        if let Some(r) = receiver.filter(|r| *r != sender) {
            self.link(r, id);
        }
        id
    }

    fn remove(&mut self, id: ConnectionId) -> Option<Connection> {
        let c = self.conns.remove(id)?;
        self.unlink(c.sender, id);
        if let Some(r) = c.receiver {
            self.unlink(r, id);
        }
        Some(c)
    }

    /// Drops every connection that involves one of `objects`.
    pub(crate) fn remove_objects(&mut self, objects: &[ObjectId]) -> Vec<Connection> {
        let mut out = Vec::new();
        for obj in objects {
            let Some(ids) = self.by_object.remove(*obj) else {
                continue;
            };
            for id in ids {
                if let Some(c) = self.remove(id) {
                    out.push(c);
                }
            }
        }
        out
    }
}

impl Engine {
    pub fn signal_index(&self, obj: ObjectId, name: &str) -> Result<SignalIndex, EngineError> {
        let store = self.inner.store.borrow();
        let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
        h.meta
            .signal_index(name)
            .ok_or_else(|| EngineError::UnknownSignal {
                ty: h.meta.name.to_string(),
                name: name.into(),
            })
    }

    /// Connects `handler` to `obj`'s `signal`.
    pub fn connect(
        &self,
        obj: ObjectId,
        signal: SignalIndex,
        handler: impl Fn(&Engine, &[Value]) + 'static,
    ) -> Result<ConnectionId, EngineError> {
        self.connect_impl(obj, signal, None, Rc::new(handler))
    }

    pub fn connect_by_name(
        &self,
        obj: ObjectId,
        name: &str,
        handler: impl Fn(&Engine, &[Value]) + 'static,
    ) -> Result<ConnectionId, EngineError> {
        let s = self.signal_index(obj, name)?;
        self.connect(obj, s, handler)
    }

    /// Like [`Engine::connect`], but dropped when `receiver` is destroyed
    /// too (a `Connections` element, a handler in another component).
    pub fn connect_to(
        &self,
        obj: ObjectId,
        signal: SignalIndex,
        receiver: ObjectId,
        handler: impl Fn(&Engine, &[Value]) + 'static,
    ) -> Result<ConnectionId, EngineError> {
        if !self.is_alive(receiver) {
            return Err(EngineError::DeadObject(receiver));
        }
        self.connect_impl(obj, signal, Some(receiver), Rc::new(handler))
    }

    fn connect_impl(
        &self,
        obj: ObjectId,
        signal: SignalIndex,
        receiver: Option<ObjectId>,
        handler: Handler,
    ) -> Result<ConnectionId, EngineError> {
        let notify_of = {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
            let s = h
                .meta
                .signal(signal)
                .ok_or_else(|| EngineError::UnknownSignal {
                    ty: h.meta.name.to_string(),
                    name: format!("#{}", signal.0),
                })?;
            s.notify_of
        };
        let watch = match notify_of {
            Some(prop) => {
                let n = self.ensure_node(obj, prop)?;
                let scope = self.ensure_scope(obj)?;
                let weak = Rc::downgrade(&self.inner);
                let h = handler.clone();
                Some(self.inner.graph.watch(scope, n, move |_g, _v| {
                    if let Some(inner) = weak.upgrade() {
                        let engine = Engine::from_inner(inner);
                        let _t = Tracking::pause(&engine.inner.tracking);
                        h(&engine, &[]);
                    }
                })?)
            }
            None => None,
        };
        Ok(self.inner.signals.borrow_mut().insert(Connection {
            sender: obj,
            signal,
            receiver,
            handler,
            watch,
        }))
    }

    /// Removes a connection. Returns false if it was already gone. A
    /// handler disconnected during an emission is not called afterwards.
    pub fn disconnect(&self, id: ConnectionId) -> bool {
        let c = self.inner.signals.borrow_mut().remove(id);
        match c {
            Some(c) => {
                if let Some(w) = c.watch {
                    let _ = self.inner.graph.dispose(w);
                }
                drop(c);
                true
            }
            None => false,
        }
    }

    /// Emits `signal` on `obj`: calls its handlers now, in connection
    /// order. Arguments are coerced to the declared parameter types where
    /// they can be. Emitting a change signal notifies the property.
    pub fn emit(
        &self,
        obj: ObjectId,
        signal: SignalIndex,
        args: &[Value],
    ) -> Result<(), EngineError> {
        let (notify_of, args) = {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
            let s = h
                .meta
                .signal(signal)
                .ok_or_else(|| EngineError::UnknownSignal {
                    ty: h.meta.name.to_string(),
                    name: format!("#{}", signal.0),
                })?;
            let mut out: SmallVec<[Value; 4]> = args.iter().cloned().collect();
            for (i, (_, ty)) in s.params.iter().enumerate() {
                match out.get_mut(i) {
                    Some(v) => {
                        if let Ok(c) = ty.coerce(v) {
                            *v = c;
                        }
                    }
                    None => out.push(Value::Undefined),
                }
            }
            (s.notify_of, out)
        };
        if let Some(prop) = notify_of {
            return self.notify(obj, prop);
        }
        let ids: SmallVec<[ConnectionId; 4]> = {
            let signals = self.inner.signals.borrow();
            match signals.by_object.get(obj) {
                Some(ids) => ids
                    .iter()
                    .copied()
                    .filter(|id| {
                        signals
                            .conns
                            .get(*id)
                            .is_some_and(|c| c.sender == obj && c.signal == signal)
                    })
                    .collect(),
                None => return Ok(()),
            }
        };
        for id in ids {
            let handler = self
                .inner
                .signals
                .borrow()
                .conns
                .get(id)
                .map(|c| c.handler.clone());
            if let Some(h) = handler {
                self.untrack(|| h(self, &args));
            }
        }
        Ok(())
    }

    pub fn emit_by_name(
        &self,
        obj: ObjectId,
        name: &str,
        args: &[Value],
    ) -> Result<(), EngineError> {
        let s = self.signal_index(obj, name)?;
        self.emit(obj, s, args)
    }

    /// Emits a signal of the native type `owner` (generated `emit_*`
    /// helpers), checking `obj` is one.
    pub fn emit_typed(
        &self,
        obj: ObjectId,
        owner: &MetaObject,
        signal: SignalIndex,
        args: &[Value],
    ) -> Result<(), EngineError> {
        self.check_owner(obj, owner)?;
        self.emit(obj, signal, args)
    }

    /// Whether anything is connected to `obj`'s `signal`. For a signal
    /// whose event counts as taken once a handler has seen it (the
    /// per-key signals of `Keys`), where "no handler" must not take it.
    pub fn signal_connected(&self, obj: ObjectId, signal: SignalIndex) -> bool {
        let signals = self.inner.signals.borrow();
        signals.by_object.get(obj).is_some_and(|ids| {
            ids.iter().any(|id| {
                signals
                    .conns
                    .get(*id)
                    .is_some_and(|c| c.sender == obj && c.signal == signal)
            })
        })
    }

    /// Number of live connections.
    pub fn connection_count(&self) -> usize {
        self.inner.signals.borrow().conns.len()
    }
}
