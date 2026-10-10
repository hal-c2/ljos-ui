//! The [`Engine`]: object store, reactive properties, contexts, registry.
//!
//! # Borrows
//!
//! State is split into separately borrowed parts (store, signals, contexts,
//! registry; the graph has its own). Every method borrows for as long as it
//! touches plain data and releases before anything that can run user code:
//! a graph read (which can evaluate a binding), a write (which flushes
//! effects), a signal handler, a native method. So a binding may read other
//! objects, write elsewhere, emit, create and destroy without tripping a
//! `RefCell`.
//!
//! # Storage
//!
//! A property's value lives in its native field or dynamic slot. Once the
//! property is observed (read inside a binding or effect) or bound, it also
//! gets a reactive node, which is then authoritative: the field becomes a
//! write-through cache, updated on every write and every evaluation of its
//! binding. Bindings are lazy, so a bound field is current only once
//! something has read the property; native code that needs live values
//! reads through the engine ([`Engine::get`], [`Cx::get`]) or calls
//! [`Engine::refresh_fields`] first.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_reactive::{
    Diagnostic, Graph, NodeId, Options, OverrideId, OverrideValue, Provenance, RestoreMode,
    ScopeId, WriteKind,
};
use slotmap::{Key, SecondaryMap, SlotMap};
use smallvec::SmallVec;

use crate::atom::Atom;
use crate::context::Context;
use crate::cx::Cx;
use crate::error::EngineError;
use crate::ids::{ContextId, MethodIndex, ObjectId, PropIndex, PropKey};
use crate::log::LogState;
use crate::meta::{CompleteThunk, MetaObject, MetaRef, Prop, PropertyAccess, PropertyFlags};
use crate::object::{
    AliasTarget, ObjectFlags, ObjectHeader, QmlObject, QmlType, downcast_chain, downcast_chain_mut,
    find_native, find_native_mut,
};
use crate::registry::{ModuleDef, Registry, TypeDump, Version};
use crate::signal::Signals;
use crate::types::{ConvertError, QmlValue, TypeTag};
use crate::value::Value;

/// Called when an object's visual parent changes: (object, new parent).
/// A binding's evaluator; `None` keeps the old value (it threw).
pub(crate) type BindingFn = Box<dyn FnMut(&Engine) -> Option<Value>>;

pub type ParentListener = Rc<dyn Fn(&Engine, ObjectId, Option<ObjectId>)>;

pub(crate) struct Store {
    pub(crate) objects: SlotMap<ObjectId, ObjectHeader>,
    /// Node to property, for diagnostics.
    pub(crate) node_keys: SecondaryMap<NodeId, PropKey>,
    pub(crate) deferred: Vec<ObjectId>,
}

pub(crate) struct Inner {
    pub(crate) graph: Graph<Value>,
    pub(crate) store: RefCell<Store>,
    pub(crate) signals: RefCell<Signals>,
    contexts: RefCell<SlotMap<ContextId, Context>>,
    root_context: ContextId,
    registry: RefCell<Registry>,
    /// Depth of engine-run bindings and effects on the stack; 0 inside
    /// handlers and `untrack`. A read creates a node only when this is
    /// non-zero, because only then is there a reader to depend on it.
    pub(crate) tracking: Cell<u32>,
    pub(crate) log: RefCell<LogState>,
    /// The JS host, made on first use.
    pub(crate) js: RefCell<Option<Rc<crate::js::JsState>>>,
    pub(crate) loader: RefCell<crate::loader::LoaderState>,
    pub(crate) timers: RefCell<crate::qtqml::TimerState>,
    /// Nodes whose binding loop was reported this turn: a loop re-runs
    /// until the graph's re-run limit, and Qt says it once.
    pub(crate) loops_reported: RefCell<std::collections::HashSet<NodeId>>,
    /// Evaluate bindings at creation instead of on first read.
    pub(crate) eager_bindings: Cell<bool>,
    /// Read by every translated string; bumped by
    /// [`Engine::retranslate`].
    pub(crate) translation: Cell<Option<NodeId>>,
    pub(crate) parent_listeners: RefCell<Vec<ParentListener>>,
}

/// Restores the tracking depth when dropped (also on unwind).
pub(crate) struct Tracking<'a> {
    cell: &'a Cell<u32>,
    prev: u32,
}

impl<'a> Tracking<'a> {
    pub(crate) fn enter(cell: &'a Cell<u32>) -> Self {
        let prev = cell.get();
        cell.set(prev + 1);
        Tracking { cell, prev }
    }

    pub(crate) fn pause(cell: &'a Cell<u32>) -> Self {
        let prev = cell.replace(0);
        Tracking { cell, prev }
    }
}

impl Drop for Tracking<'_> {
    fn drop(&mut self) {
        self.cell.set(self.prev);
    }
}

/// The engine. One per UI thread; `!Send` (it holds `Rc`s and the graph).
///
/// Closures the engine installs (bindings, effects, handlers) receive
/// `&Engine` and may use the whole API. Cloning makes another handle to the
/// same engine.
#[derive(Clone)]
pub struct Engine {
    pub(crate) inner: Rc<Inner>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("objects", &self.object_count())
            .field("nodes", &self.inner.graph.len())
            .finish()
    }
}

/// An override (`Binding {}`, `PropertyChanges`) content.
pub enum OverrideContent {
    Value(Value),
    Binding(Box<dyn FnMut(&Engine) -> Value>),
}

impl std::fmt::Debug for OverrideContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OverrideContent::Value(v) => f.debug_tuple("Value").field(v).finish(),
            OverrideContent::Binding(_) => f.write_str("Binding(..)"),
        }
    }
}

impl Engine {
    pub fn new() -> Self {
        let graph = Graph::with_options(Options {
            equals: Value::change_equals,
            ..Options::default()
        });
        let mut contexts = SlotMap::with_key();
        let root_context = contexts.insert(Context::default());
        let inner = Rc::new(Inner {
            graph,
            store: RefCell::new(Store {
                objects: SlotMap::with_key(),
                node_keys: SecondaryMap::new(),
                deferred: Vec::new(),
            }),
            signals: RefCell::new(Signals::default()),
            contexts: RefCell::new(contexts),
            root_context,
            registry: RefCell::new(Registry::new()),
            tracking: Cell::new(0),
            log: RefCell::new(LogState::default()),
            js: RefCell::new(None),
            loader: RefCell::new(crate::loader::LoaderState::default()),
            timers: RefCell::new(crate::qtqml::TimerState::default()),
            loops_reported: RefCell::new(std::collections::HashSet::new()),
            eager_bindings: Cell::new(false),
            translation: Cell::new(None),
            parent_listeners: RefCell::new(Vec::new()),
        });
        let weak = Rc::downgrade(&inner);
        inner.graph.set_diagnostic_handler(move |d| {
            if let Some(inner) = weak.upgrade() {
                Engine { inner }.report(d);
            }
        });
        let engine = Engine { inner };
        crate::qtqml::register(&engine);
        engine
    }

    /// Whether `a` and `b` are handles to the same engine.
    pub fn ptr_eq(a: &Engine, b: &Engine) -> bool {
        Rc::ptr_eq(&a.inner, &b.inner)
    }

    /// Rebuilds a handle inside a closure the engine installed.
    pub(crate) fn from_inner(inner: Rc<Inner>) -> Engine {
        Engine { inner }
    }

    /// The reactive graph, for runtime crates (freezing scopes, stats).
    pub fn graph(&self) -> &Graph<Value> {
        &self.inner.graph
    }

    // ------------------------------------------------------------ registry

    pub fn register_module(&self, def: ModuleDef) {
        self.inner.registry.borrow_mut().register_module(def);
    }

    pub fn register_type(&self, uri: &str, version: Version, meta: MetaRef) {
        self.inner
            .registry
            .borrow_mut()
            .register_type(uri, version, meta);
    }

    pub fn lookup_type(&self, uri: &str, name: &str) -> Option<MetaRef> {
        self.inner.registry.borrow().lookup(uri, name)
    }

    /// `name` in any registered module.
    pub fn lookup_type_any(&self, name: &str) -> Option<MetaRef> {
        self.inner.registry.borrow().lookup_any(name)
    }

    pub fn has_module(&self, uri: &str) -> bool {
        self.inner.registry.borrow().has_module(uri)
    }

    pub fn lookup_type_versioned(
        &self,
        uri: &str,
        version: Version,
        name: &str,
    ) -> Option<MetaRef> {
        self.inner
            .registry
            .borrow()
            .lookup_versioned(uri, version, name)
    }

    /// The `ljostypes.json` content for everything registered.
    pub fn type_dump(&self) -> TypeDump {
        self.inner.registry.borrow().dump()
    }

    // ------------------------------------------------------------ warnings

    fn report(&self, d: &Diagnostic) {
        let node = match d {
            Diagnostic::BindingLoop { node, .. } | Diagnostic::RerunLimit { node, .. } => *node,
        };
        let first = match d {
            Diagnostic::BindingLoop { .. } => self.inner.loops_reported.borrow_mut().insert(node),
            Diagnostic::RerunLimit { .. } => !self.inner.loops_reported.borrow().contains(&node),
        };
        if !first {
            return;
        }
        let key = self.inner.store.borrow().node_keys.get(node).copied();
        let what = key.map(|k| self.describe(k.object, k.prop));
        let msg = match (d, what) {
            (Diagnostic::BindingLoop { .. }, Some((ty, prop))) => {
                let k = key.expect("described");
                match self.binding_location(k.object, k.prop) {
                    // A property declared in QML: the object, then the
                    // declaration on a second line.
                    Some((obj_loc, binding_loc, true)) => format!(
                        "{obj_loc}: QML {ty}: Binding loop detected for property \"{prop}\":\n{binding_loc}"
                    ),
                    // A native property: where the binding is written.
                    Some((_, binding_loc, false)) => format!(
                        "{binding_loc}: QML {ty}: Binding loop detected for property \"{prop}\""
                    ),
                    None => format!("QML {ty}: Binding loop detected for property \"{prop}\""),
                }
            }
            (Diagnostic::RerunLimit { runs, .. }, Some((ty, prop))) => {
                format!("QML {ty}: property \"{prop}\" handlers re-ran {runs} times in one update")
            }
            (d, None) => format!("QML: {d}"),
        };
        self.warn(msg);
    }

    /// `url:line:col` of the object and of the binding on `prop`, and
    /// whether the property is declared in QML, when the object was made
    /// from QML.
    fn binding_location(&self, obj: ObjectId, prop: PropIndex) -> Option<(String, String, bool)> {
        let store = self.inner.store.borrow();
        let ext = store.objects.get(obj)?.ext.as_ref()?;
        let loc = ext.location.as_ref()?;
        let (_, line, col, declared) = ext.binding_locs.iter().find(|(p, ..)| *p == prop)?;
        Some((
            format!("{}:{}:{}", loc.url, loc.line, loc.column),
            format!("{}:{line}:{col}", loc.url),
            *declared,
        ))
    }

    /// Type and property name, for messages.
    pub(crate) fn describe(&self, obj: ObjectId, prop: PropIndex) -> (Atom, Atom) {
        let store = self.inner.store.borrow();
        match store.objects.get(obj) {
            Some(h) => (
                h.meta.name.clone(),
                h.meta
                    .property(prop)
                    .map_or_else(|| Atom::intern("?"), |p| p.name.clone()),
            ),
            None => (Atom::intern("?"), Atom::intern("?")),
        }
    }

    // ------------------------------------------------------------- objects

    /// Makes an object of `meta` with its constructor. `owner` destroys it.
    pub fn create(
        &self,
        meta: impl Into<MetaRef>,
        owner: Option<ObjectId>,
    ) -> Result<ObjectId, EngineError> {
        let meta = meta.into();
        let make = meta
            .create
            .ok_or_else(|| EngineError::NotCreatable(meta.name.to_string()))?;
        self.create_with(meta, make(), owner)
    }

    /// Makes an object of a registered type.
    pub fn create_by_name(
        &self,
        uri: &str,
        name: &str,
        owner: Option<ObjectId>,
    ) -> Result<ObjectId, EngineError> {
        let meta = self
            .lookup_type(uri, name)
            .ok_or_else(|| EngineError::UnknownType {
                uri: uri.into(),
                name: name.into(),
            })?;
        self.create(meta, owner)
    }

    /// Adds a native value built in Rust.
    pub fn create_native<T: QmlType>(
        &self,
        native: T,
        owner: Option<ObjectId>,
    ) -> Result<ObjectId, EngineError> {
        self.create_with(MetaRef::Static(T::static_meta()), Box::new(native), owner)
    }

    /// Adds an object whose description is `meta` (a QML type, say) and
    /// whose native part is `native`, which must be the native type `meta`
    /// is built on.
    pub fn create_with(
        &self,
        meta: MetaRef,
        native: Box<dyn QmlObject>,
        owner: Option<ObjectId>,
    ) -> Result<ObjectId, EngineError> {
        if !meta.inherits(native.meta()) {
            return Err(EngineError::WrongType {
                ty: meta.name.to_string(),
                expected: native.meta().name.to_string(),
            });
        }
        let mut store = self.inner.store.borrow_mut();
        let context = match owner {
            Some(o) => {
                store
                    .objects
                    .get(o)
                    .ok_or(EngineError::DeadObject(o))?
                    .context
            }
            None => self.inner.root_context,
        };
        let mut header = ObjectHeader::new(meta, native, context);
        header.owner = owner;
        let id = store.objects.insert(header);
        if let Some(o) = owner {
            store.objects[o].children.push(id);
        }
        Ok(id)
    }

    pub fn is_alive(&self, obj: ObjectId) -> bool {
        self.inner.store.borrow().objects.contains_key(obj)
    }

    pub fn object_count(&self) -> usize {
        self.inner.store.borrow().objects.len()
    }

    pub fn meta_of(&self, obj: ObjectId) -> Option<MetaRef> {
        Some(self.inner.store.borrow().objects.get(obj)?.meta.clone())
    }

    pub fn flags(&self, obj: ObjectId) -> Option<ObjectFlags> {
        Some(self.inner.store.borrow().objects.get(obj)?.flags)
    }

    /// Schedules `obj` (and what it owns) for destruction at the next
    /// [`Engine::process_deferred_deletes`], like `destroy()` in QML.
    /// Returns false if it is already gone.
    pub fn destroy(&self, obj: ObjectId) -> bool {
        let mut store = self.inner.store.borrow_mut();
        let Some(h) = store.objects.get_mut(obj) else {
            return false;
        };
        if !h.flags.contains(ObjectFlags::PENDING_DELETE) {
            h.flags |= ObjectFlags::PENDING_DELETE;
            store.deferred.push(obj);
        }
        true
    }

    /// Destroys everything scheduled by [`Engine::destroy`]. Returns how
    /// many scheduled objects were destroyed.
    pub fn process_deferred_deletes(&self) -> usize {
        let mut n = 0;
        loop {
            let batch = std::mem::take(&mut self.inner.store.borrow_mut().deferred);
            if batch.is_empty() {
                return n;
            }
            for obj in batch {
                n += usize::from(self.destroy_now(obj));
            }
        }
    }

    /// Destroys `obj` and everything it owns, children first. Its reactive
    /// nodes are disposed, so bindings that read it re-run and see `null`;
    /// its connections, as sender or receiver, are dropped.
    pub fn destroy_now(&self, obj: ObjectId) -> bool {
        // Post-order: children before their owner.
        let order = {
            let store = self.inner.store.borrow();
            if !store.objects.contains_key(obj) {
                return false;
            }
            let mut order = Vec::new();
            let mut stack = vec![(obj, false)];
            while let Some((id, expanded)) = stack.pop() {
                if expanded {
                    order.push(id);
                    continue;
                }
                stack.push((id, true));
                if let Some(h) = store.objects.get(id) {
                    stack.extend(h.children.iter().rev().map(|c| (*c, false)));
                }
            }
            order
        };
        let mut headers = Vec::with_capacity(order.len());
        // Survivors whose visual parent died, to tell listeners.
        let mut orphans = Vec::new();
        {
            let mut store = self.inner.store.borrow_mut();
            let s = &mut *store;
            for id in &order {
                let Some(mut h) = s.objects.remove(*id) else {
                    continue;
                };
                h.flags |= ObjectFlags::DESTROYING;
                if let Some(nodes) = &h.nodes {
                    for n in nodes.iter().flatten() {
                        s.node_keys.remove(*n);
                    }
                }
                headers.push(h);
            }
            // Unlink survivors from the dead.
            for h in &headers {
                if let Some(o) = h.owner.and_then(|o| s.objects.get_mut(o)) {
                    o.children.retain(|c| *c != obj);
                }
                if let Some(p) = h.visual_parent.and_then(|p| s.objects.get_mut(p)) {
                    p.visual_children.retain(|c| not_in(&order, *c));
                }
                for c in &h.visual_children {
                    if let Some(ch) = s.objects.get_mut(*c) {
                        ch.visual_parent = None;
                        orphans.push(*c);
                    }
                }
            }
        }
        let connections = self.inner.signals.borrow_mut().remove_objects(&order);
        for c in &connections {
            if let Some(w) = c.watch {
                let _ = self.inner.graph.dispose(w);
            }
        }
        crate::qtqml::forget_timers(self, &order);
        let js = self.inner.js.borrow().clone();
        if let Some(js) = js {
            js.forget(&order);
        }
        self.inner.graph.batch(|| {
            for h in &headers {
                if let Some(scope) = h.scope {
                    let _ = self.inner.graph.dispose_scope(scope);
                }
            }
            for c in &orphans {
                self.parent_changed(*c, None);
            }
        });
        for h in &headers {
            if let Some(ext) = &h.ext {
                for c in &ext.contexts {
                    self.destroy_context(*c);
                }
            }
        }
        // Natives and handlers may own anything; drop them unborrowed.
        drop(connections);
        drop(headers);
        true
    }

    // ------------------------------------------------------------ hierarchy

    /// The owner (QObject parent).
    pub fn owner(&self, obj: ObjectId) -> Option<ObjectId> {
        self.inner.store.borrow().objects.get(obj)?.owner
    }

    /// Owned children, in creation order.
    pub fn children(&self, obj: ObjectId) -> Vec<ObjectId> {
        self.inner
            .store
            .borrow()
            .objects
            .get(obj)
            .map_or_else(Vec::new, |h| h.children.clone())
    }

    /// Moves `obj` to a new owner. Fails on a dead object or if `owner` is
    /// owned by `obj` (a cycle).
    pub fn set_owner(&self, obj: ObjectId, owner: Option<ObjectId>) -> Result<(), EngineError> {
        let mut store = self.inner.store.borrow_mut();
        if !store.objects.contains_key(obj) {
            return Err(EngineError::DeadObject(obj));
        }
        if let Some(o) = owner {
            let mut cur = Some(o);
            while let Some(c) = cur {
                let h = store.objects.get(c).ok_or(EngineError::DeadObject(c))?;
                if c == obj {
                    return Err(EngineError::WrongType {
                        ty: "owner".into(),
                        expected: "an object not owned by the child".into(),
                    });
                }
                cur = h.owner;
            }
        }
        let old = std::mem::replace(&mut store.objects[obj].owner, owner);
        if let Some(h) = old.and_then(|o| store.objects.get_mut(o)) {
            h.children.retain(|c| *c != obj);
        }
        if let Some(o) = owner {
            store.objects[o].children.push(obj);
        }
        Ok(())
    }

    /// The Item parent. Inside a binding or effect the read is tracked:
    /// it re-runs when [`Engine::set_visual_parent`] changes it.
    pub fn visual_parent(&self, obj: ObjectId) -> Option<ObjectId> {
        if self.inner.tracking.get() > 0
            && let Ok(n) = self.parent_node(obj)
        {
            return match self.inner.graph.try_get(n) {
                Some(Value::Object(p)) if self.is_alive(p) => Some(p),
                _ => None,
            };
        }
        self.inner.store.borrow().objects.get(obj)?.visual_parent
    }

    pub fn visual_children(&self, obj: ObjectId) -> Vec<ObjectId> {
        self.inner
            .store
            .borrow()
            .objects
            .get(obj)
            .map_or_else(Vec::new, |h| h.visual_children.clone())
    }

    /// Sets the Item parent, which does not change ownership. Observers of
    /// [`Engine::visual_parent`] and listeners added with
    /// [`Engine::on_visual_parent_changed`] hear of it.
    pub fn set_visual_parent(
        &self,
        obj: ObjectId,
        parent: Option<ObjectId>,
    ) -> Result<(), EngineError> {
        {
            let mut store = self.inner.store.borrow_mut();
            if !store.objects.contains_key(obj) {
                return Err(EngineError::DeadObject(obj));
            }
            if let Some(p) = parent
                && !store.objects.contains_key(p)
            {
                return Err(EngineError::DeadObject(p));
            }
            let old = std::mem::replace(&mut store.objects[obj].visual_parent, parent);
            if old == parent {
                return Ok(());
            }
            if let Some(h) = old.and_then(|o| store.objects.get_mut(o)) {
                h.visual_children.retain(|c| *c != obj);
            }
            if let Some(p) = parent {
                store.objects[p].visual_children.push(obj);
            }
        }
        self.inner.graph.batch(|| self.parent_changed(obj, parent));
        Ok(())
    }

    /// Calls `f` after every visual-parent change, with the object and its
    /// new parent (a scene reparents render nodes here).
    pub fn on_visual_parent_changed(
        &self,
        f: impl Fn(&Engine, ObjectId, Option<ObjectId>) + 'static,
    ) {
        self.inner.parent_listeners.borrow_mut().push(Rc::new(f));
    }

    fn parent_changed(&self, obj: ObjectId, parent: Option<ObjectId>) {
        let node = self
            .inner
            .store
            .borrow()
            .objects
            .get(obj)
            .and_then(|h| h.ext.as_ref()?.parent_node);
        if let Some(n) = node {
            let v = parent.map_or(Value::Null, Value::Object);
            let _ = self.inner.graph.write(n, v, WriteKind::Initial);
        }
        let listeners = self.inner.parent_listeners.borrow().clone();
        for l in listeners {
            l(self, obj, parent);
        }
    }

    fn parent_node(&self, obj: ObjectId) -> Result<NodeId, EngineError> {
        let current = {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
            if let Some(n) = h.ext.as_ref().and_then(|e| e.parent_node) {
                return Ok(n);
            }
            h.visual_parent
        };
        let scope = self.ensure_scope(obj)?;
        let n = self
            .inner
            .graph
            .property(scope, current.map_or(Value::Null, Value::Object))?;
        if let Some(h) = self.inner.store.borrow_mut().objects.get_mut(obj) {
            h.ext_mut().parent_node = Some(n);
        }
        Ok(n)
    }

    /// The object's graph scope, made on first need as a child of its
    /// owner's, so the scopes follow the ownership tree.
    pub(crate) fn ensure_scope(&self, obj: ObjectId) -> Result<ScopeId, EngineError> {
        let owner = {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
            if let Some(s) = h.scope {
                return Ok(s);
            }
            h.owner
        };
        let g = &self.inner.graph;
        let parent = match owner {
            Some(o) => self.ensure_scope(o)?,
            None => g.root(),
        };
        let s = g.create_scope(parent)?;
        self.inner.store.borrow_mut().objects[obj].scope = Some(s);
        Ok(s)
    }

    /// Makes a tracked reader depend on `obj` staying alive.
    fn track_alive(&self, obj: ObjectId) {
        let node = {
            let store = self.inner.store.borrow();
            match store.objects.get(obj) {
                Some(h) => h.ext.as_ref().and_then(|e| e.alive),
                None => return,
            }
        };
        let n = match node {
            Some(n) => n,
            None => {
                let Ok(scope) = self.ensure_scope(obj) else {
                    return;
                };
                let Ok(n) = self.inner.graph.property(scope, Value::Bool(true)) else {
                    return;
                };
                if let Some(h) = self.inner.store.borrow_mut().objects.get_mut(obj) {
                    h.ext_mut().alive = Some(n);
                }
                n
            }
        };
        let _ = self.inner.graph.try_get(n);
    }

    // ------------------------------------------------------------- natives

    /// Borrows the `T` part of `obj`. `f` must not call back into the
    /// engine (the store is borrowed while it runs).
    pub fn with<T: QmlType, R>(
        &self,
        obj: ObjectId,
        f: impl FnOnce(&T) -> R,
    ) -> Result<R, EngineError> {
        let store = self.inner.store.borrow();
        let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
        let t = downcast_chain::<T>(&*h.native).ok_or_else(|| EngineError::WrongType {
            ty: h.meta.name.to_string(),
            expected: T::static_meta().name.to_string(),
        })?;
        Ok(f(t))
    }

    /// Mutably borrows the `T` part of `obj`. Changes are not seen by
    /// observers until [`Engine::notify`]; prefer [`Engine::set_internal`].
    pub fn with_mut<T: QmlType, R>(
        &self,
        obj: ObjectId,
        f: impl FnOnce(&mut T) -> R,
    ) -> Result<R, EngineError> {
        let mut store = self.inner.store.borrow_mut();
        let h = store
            .objects
            .get_mut(obj)
            .ok_or(EngineError::DeadObject(obj))?;
        let name = h.meta.name.clone();
        let t = downcast_chain_mut::<T>(&mut *h.native).ok_or_else(|| EngineError::WrongType {
            ty: name.to_string(),
            expected: T::static_meta().name.to_string(),
        })?;
        Ok(f(t))
    }

    // ---------------------------------------------------------- properties

    pub fn property_index(&self, obj: ObjectId, name: &str) -> Result<PropIndex, EngineError> {
        let store = self.inner.store.borrow();
        let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
        h.meta
            .property_index(name)
            .ok_or_else(|| EngineError::UnknownProperty {
                ty: h.meta.name.to_string(),
                name: name.into(),
            })
    }

    /// Reads a property. Inside a binding or effect the read is tracked.
    /// A destroyed object reads as `null`, an unknown property as
    /// `undefined`.
    pub fn read(&self, obj: ObjectId, prop: PropIndex) -> Value {
        match self.try_read(obj, prop) {
            Ok(v) => v,
            Err(EngineError::DeadObject(_)) => Value::Null,
            Err(_) => Value::Undefined,
        }
    }

    pub fn read_by_name(&self, obj: ObjectId, name: &str) -> Value {
        match self.property_index(obj, name) {
            Ok(p) => self.read(obj, p),
            Err(EngineError::DeadObject(_)) => Value::Null,
            Err(_) => Value::Undefined,
        }
    }

    /// [`Engine::read`] that reports why there is no value.
    pub fn try_read(&self, obj: ObjectId, prop: PropIndex) -> Result<Value, EngineError> {
        enum Got {
            Node(NodeId),
            Value(Value),
            NeedNode,
            Alias(Option<AliasTarget>),
        }
        let got = {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
            let p = h
                .meta
                .property(prop)
                .ok_or_else(|| unknown_index(&h.meta, prop))?;
            if p.is_alias() {
                Got::Alias(h.alias(prop))
            } else {
                match h.node(prop) {
                    Some(n) => Got::Node(n),
                    None if self.inner.tracking.get() == 0
                        || p.flags
                            .intersects(PropertyFlags::CONSTANT | PropertyFlags::GROUP) =>
                    {
                        Got::Value(read_access(&*h.native, &h.dynamic, &p.access))
                    }
                    None => Got::NeedNode,
                }
            }
        };
        let v = match got {
            Got::Value(v) => v,
            Got::Node(n) => self.inner.graph.try_get(n).unwrap_or_default(),
            Got::NeedNode => {
                let n = self.ensure_node(obj, prop)?;
                self.inner.graph.try_get(n).unwrap_or_default()
            }
            Got::Alias(Some(AliasTarget {
                object,
                prop: Some(p),
            })) => return self.try_read(object, p),
            Got::Alias(Some(AliasTarget { object, prop: None })) => Value::Object(object),
            // Not wired yet (still being created).
            Got::Alias(None) => Value::Undefined,
        };
        Ok(self.live(v))
    }

    /// A reference to a destroyed object reads as `null`; a tracked read
    /// of a live one re-runs the reader when it is destroyed.
    fn live(&self, v: Value) -> Value {
        match v {
            Value::Object(id) if !self.is_alive(id) => Value::Null,
            Value::Object(id) => {
                if self.inner.tracking.get() > 0 {
                    self.track_alive(id);
                }
                v
            }
            v => v,
        }
    }

    /// Points the alias property `prop` of `obj` at `target`'s property
    /// `target_prop`, or at `target` itself.
    pub fn set_alias(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        target: ObjectId,
        target_prop: Option<PropIndex>,
    ) -> Result<(), EngineError> {
        let mut store = self.inner.store.borrow_mut();
        let h = store
            .objects
            .get_mut(obj)
            .ok_or(EngineError::DeadObject(obj))?;
        let is_alias = h.meta.property(prop).is_some_and(|p| p.is_alias());
        if !is_alias {
            return Err(EngineError::WrongType {
                ty: h.meta.name.to_string(),
                expected: "an alias property".into(),
            });
        }
        let t = AliasTarget {
            object: target,
            prop: target_prop,
        };
        if target_prop.is_none() {
            // An alias of an object never changes; its slot holds it so a
            // change-signal connection has a node to watch.
            let _ = store_in(h, prop, &Value::Object(target));
        }
        let ext = h.ext_mut();
        match ext.aliases.iter_mut().find(|(p, _)| *p == prop) {
            Some(e) => e.1 = t,
            None => ext.aliases.push((prop, t)),
        }
        Ok(())
    }

    /// Follows aliases to the property that holds the value.
    pub(crate) fn redirect(&self, obj: ObjectId, prop: PropIndex) -> (ObjectId, PropIndex) {
        let mut cur = (obj, prop);
        // Bounded: the compiler rejects alias cycles, but a bad IR must not
        // hang.
        for _ in 0..32 {
            let store = self.inner.store.borrow();
            let Some(h) = store.objects.get(cur.0) else {
                return cur;
            };
            match h.meta.property(cur.1) {
                Some(p) if p.is_alias() => match h.alias(cur.1) {
                    Some(AliasTarget {
                        object,
                        prop: Some(tp),
                    }) => cur = (object, tp),
                    _ => return cur,
                },
                _ => return cur,
            }
        }
        cur
    }

    /// Writes a property from QML/JS: read-only properties are rejected,
    /// the value is coerced to the property's type, and `how` decides what
    /// happens to a binding (see [`WriteKind`]).
    pub fn write(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        v: Value,
        how: WriteKind,
    ) -> Result<(), EngineError> {
        self.write_impl(obj, prop, v, how, false)
    }

    pub fn write_by_name(&self, obj: ObjectId, name: &str, v: Value) -> Result<(), EngineError> {
        let p = self.property_index(obj, name)?;
        self.write(obj, p, v, WriteKind::Imperative)
    }

    /// Writes from native code or the object creator: read-only and
    /// constant properties are writable.
    pub fn write_internal(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        v: Value,
        how: WriteKind,
    ) -> Result<(), EngineError> {
        self.write_impl(obj, prop, v, how, true)
    }

    fn check_writable(
        &self,
        h: &ObjectHeader,
        prop: PropIndex,
        internal: bool,
    ) -> Result<TypeTag, EngineError> {
        let p = h
            .meta
            .property(prop)
            .ok_or_else(|| unknown_index(&h.meta, prop))?;
        if p.flags.contains(PropertyFlags::GROUP) {
            return Err(EngineError::GroupProperty(p.name.to_string()));
        }
        let constant =
            p.flags.contains(PropertyFlags::CONSTANT) && h.flags.contains(ObjectFlags::COMPLETED);
        if !internal && (p.is_readonly() || constant) {
            return Err(EngineError::ReadOnly(p.name.to_string()));
        }
        Ok(p.ty.clone())
    }

    fn write_impl(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        v: Value,
        how: WriteKind,
        internal: bool,
    ) -> Result<(), EngineError> {
        let (obj, prop) = self.redirect(obj, prop);
        let (v, node) = {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
            let ty = self.check_writable(h, prop, internal)?;
            (coerce_in(&store, &ty, &v)?, h.node(prop))
        };
        let n = match (node, how) {
            (Some(n), _) => n,
            // An override entry only exists on a node.
            (None, WriteKind::Override(_)) => self.ensure_node(obj, prop)?,
            (None, _) => {
                // Nobody observes it: the field is the only copy.
                let mut store = self.inner.store.borrow_mut();
                let h = store
                    .objects
                    .get_mut(obj)
                    .ok_or(EngineError::DeadObject(obj))?;
                store_in(h, prop, &v)?;
                h.mark_assigned(prop);
                return Ok(());
            }
        };
        let g = &self.inner.graph;
        // Batched so effects run after the field cache is updated.
        g.batch(|| {
            g.write(n, v, how)?;
            if let Some(cur) = g.untrack(|| g.try_get(n)) {
                self.store_value(obj, prop, &cur);
            }
            Ok(())
        })
    }

    /// Updates the field cache; a no-op for a dead object.
    pub(crate) fn store_value(&self, obj: ObjectId, prop: PropIndex, v: &Value) {
        let mut store = self.inner.store.borrow_mut();
        if let Some(h) = store.objects.get_mut(obj) {
            let _ = store_in(h, prop, v);
        }
    }

    /// Reads a native property through its typed handle.
    pub fn get<T: QmlValue>(&self, obj: ObjectId, p: Prop<T>) -> Result<T, EngineError> {
        self.check_owner(obj, (p.owner)())?;
        Ok(T::from_value(&self.try_read(obj, p.index)?)?)
    }

    /// Writes a native property through its typed handle, as QML would.
    pub fn set<T: QmlValue>(&self, obj: ObjectId, p: Prop<T>, v: T) -> Result<(), EngineError> {
        self.check_owner(obj, (p.owner)())?;
        self.write(obj, p.index, v.to_value(), WriteKind::Imperative)
    }

    /// [`Engine::set`] from native code: read-only properties are writable.
    pub fn set_internal<T: QmlValue>(
        &self,
        obj: ObjectId,
        p: Prop<T>,
        v: T,
    ) -> Result<(), EngineError> {
        self.check_owner(obj, (p.owner)())?;
        self.write_internal(obj, p.index, v.to_value(), WriteKind::Imperative)
    }

    pub(crate) fn check_owner(&self, obj: ObjectId, owner: &MetaObject) -> Result<(), EngineError> {
        let store = self.inner.store.borrow();
        let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
        if h.meta.inherits(owner) {
            Ok(())
        } else {
            Err(EngineError::WrongType {
                ty: h.meta.name.to_string(),
                expected: owner.name.to_string(),
            })
        }
    }

    /// The node of a property, made on first need. Its initial value is the
    /// field's.
    pub(crate) fn ensure_node(
        &self,
        obj: ObjectId,
        prop: PropIndex,
    ) -> Result<NodeId, EngineError> {
        let (obj, prop) = self.redirect(obj, prop);
        let (scope, value, assigned) = {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
            if let Some(n) = h.node(prop) {
                return Ok(n);
            }
            let p = h
                .meta
                .property(prop)
                .ok_or_else(|| unknown_index(&h.meta, prop))?;
            (
                h.scope,
                read_access(&*h.native, &h.dynamic, &p.access),
                h.is_assigned(prop),
            )
        };
        let g = &self.inner.graph;
        let scope = match scope {
            Some(s) => s,
            None => self.ensure_scope(obj)?,
        };
        let n = g.property(scope, value.clone())?;
        {
            let mut store = self.inner.store.borrow_mut();
            store.objects[obj].set_node(prop, n);
            store.node_keys.insert(n, PropKey { object: obj, prop });
        }
        if assigned {
            // Carries the provenance over; the value is unchanged.
            g.write(n, value, WriteKind::Initial)?;
        }
        Ok(n)
    }

    /// Installs a binding (`Qt.binding()`, or a binding from the creator).
    /// `f` runs lazily, when the property is read; its reads are tracked.
    /// The result is coerced to the property's type; on failure the
    /// property keeps its value and a warning is logged, as in Qt.
    pub fn bind(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        f: impl FnMut(&Engine) -> Value + 'static,
    ) -> Result<(), EngineError> {
        let mut f = f;
        self.bind_impl(obj, prop, Box::new(move |e| Some(f(e))), false)
    }

    /// [`Engine::bind`] for the object creator: allowed on read-only
    /// properties, whose initial binding is set by their declaration.
    pub fn bind_initial(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        f: impl FnMut(&Engine) -> Value + 'static,
    ) -> Result<(), EngineError> {
        let mut f = f;
        self.bind_impl(obj, prop, Box::new(move |e| Some(f(e))), true)
    }

    /// [`Engine::bind_initial`] for a binding that can fail (a JS
    /// exception): `None` keeps the property's last value, as Qt does.
    pub fn bind_fallible(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        f: impl FnMut(&Engine) -> Option<Value> + 'static,
    ) -> Result<(), EngineError> {
        self.bind_impl(obj, prop, Box::new(f), true)
    }

    /// [`Engine::bind_fallible`] that checks the property is writable
    /// (`Qt.binding` from JS).
    pub fn bind_fallible_checked(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        f: impl FnMut(&Engine) -> Option<Value> + 'static,
    ) -> Result<(), EngineError> {
        self.bind_impl(obj, prop, Box::new(f), false)
    }

    fn bind_impl(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        f: BindingFn,
        internal: bool,
    ) -> Result<(), EngineError> {
        let (obj, prop) = self.redirect(obj, prop);
        let ty = {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
            self.check_writable(h, prop, internal)?
        };
        let n = self.ensure_node(obj, prop)?;
        let last = self
            .inner
            .graph
            .untrack(|| self.inner.graph.try_get(n))
            .unwrap_or_default();
        let wrapper = self.binding_wrapper(obj, prop, ty, last, f);
        self.inner.graph.bind(n, wrapper)?;
        if self.inner.eager_bindings.get() {
            self.inner.graph.set_eager(n, true)?;
        }
        Ok(())
    }

    /// Evaluates bindings as they are installed and after every change
    /// instead of when first read. Off by default; tests that expect Qt's
    /// eager behaviour (binding-loop warnings at creation) turn it on.
    pub fn set_eager_bindings(&self, eager: bool) {
        self.inner.eager_bindings.set(eager);
    }

    pub fn eager_bindings(&self) -> bool {
        self.inner.eager_bindings.get()
    }

    /// Wraps a user binding for the graph: runs it tracked, coerces the
    /// result, keeps the last good value on failure and writes the field
    /// cache.
    fn binding_wrapper(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        ty: TypeTag,
        mut last: Value,
        mut f: BindingFn,
    ) -> impl FnMut(&Graph<Value>) -> Value + 'static {
        let weak = Rc::downgrade(&self.inner);
        move |_g| {
            let Some(inner) = weak.upgrade() else {
                return last.clone();
            };
            let engine = Engine::from_inner(inner);
            let v = {
                let _t = Tracking::enter(&engine.inner.tracking);
                f(&engine)
            };
            let Some(v) = v else {
                return last.clone();
            };
            let coerced = {
                let store = engine.inner.store.borrow();
                coerce_in(&store, &ty, &v)
            };
            match coerced {
                Ok(v) => last = v,
                Err(e) => {
                    let (t, p) = engine.describe(obj, prop);
                    engine.warn(format!("QML {t}: {e} (property \"{p}\")"));
                }
            }
            engine.store_value(obj, prop, &last);
            last.clone()
        }
    }

    /// Tells observers that a field changed behind the engine's back (a
    /// native method mutated it through [`Engine::with_mut`]), like
    /// emitting `xChanged` in C++. Observers re-run even if the value is
    /// equal.
    pub fn notify(&self, obj: ObjectId, prop: PropIndex) -> Result<(), EngineError> {
        let (obj, prop) = self.redirect(obj, prop);
        let (node, cur) = {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
            let p = h
                .meta
                .property(prop)
                .ok_or_else(|| unknown_index(&h.meta, prop))?;
            (h.node(prop), read_access(&*h.native, &h.dynamic, &p.access))
        };
        let Some(n) = node else {
            // Nothing observes it.
            return Ok(());
        };
        let g = &self.inner.graph;
        if g.is_bound(n) || g.override_count(n) > 0 {
            // The binding (or override) owns the value; just re-run readers.
            g.notify(n)?;
            return Ok(());
        }
        let old = g.untrack(|| g.try_get(n));
        if old.is_some_and(|o| o.change_equals(&cur)) {
            g.notify(n)?;
        } else {
            g.write(n, cur, WriteKind::Initial)?;
        }
        Ok(())
    }

    /// Evaluates the object's bound properties so its fields are current.
    pub fn refresh_fields(&self, obj: ObjectId) {
        let nodes: Vec<NodeId> = {
            let store = self.inner.store.borrow();
            match store.objects.get(obj).and_then(|h| h.nodes.as_ref()) {
                Some(n) => n.iter().flatten().copied().collect(),
                None => return,
            }
        };
        let g = &self.inner.graph;
        for n in nodes {
            if g.is_bound(n) {
                g.untrack(|| g.try_get(n));
            }
        }
    }

    /// Where the property's current value came from.
    pub fn provenance(&self, obj: ObjectId, prop: PropIndex) -> Option<Provenance> {
        let (obj, prop) = self.redirect(obj, prop);
        let (node, assigned) = {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj)?;
            (h.node(prop), h.is_assigned(prop))
        };
        match node {
            Some(n) => self.inner.graph.provenance(n),
            None if assigned => Some(Provenance::Assigned),
            None => Some(Provenance::Default),
        }
    }

    /// Whether a binding is active on the property.
    pub fn is_bound(&self, obj: ObjectId, prop: PropIndex) -> bool {
        let (obj, prop) = self.redirect(obj, prop);
        let node = self
            .inner
            .store
            .borrow()
            .objects
            .get(obj)
            .and_then(|h| h.node(prop));
        node.is_some_and(|n| self.inner.graph.is_bound(n))
    }

    /// Pushes an override (`Binding {}`, `PropertyChanges`).
    pub fn push_override(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        content: OverrideContent,
    ) -> Result<OverrideId, EngineError> {
        let (obj, prop) = self.redirect(obj, prop);
        let ty = {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
            self.check_writable(h, prop, true)?
        };
        let n = self.ensure_node(obj, prop)?;
        let g = &self.inner.graph;
        let value = match content {
            OverrideContent::Value(v) => {
                let v = coerce_in(&self.inner.store.borrow(), &ty, &v)?;
                OverrideValue::Value(v)
            }
            OverrideContent::Binding(mut f) => {
                let last = g.untrack(|| g.try_get(n)).unwrap_or_default();
                let f: BindingFn = Box::new(move |e| Some(f(e)));
                OverrideValue::binding(self.binding_wrapper(obj, prop, ty, last, f))
            }
        };
        let id = g.batch(|| {
            let id = g.push_override(n, value)?;
            self.sync_field(obj, prop, n);
            Ok::<_, EngineError>(id)
        })?;
        Ok(id)
    }

    /// Removes an override; `mode` is `Binding.restoreMode`.
    pub fn remove_override(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        id: OverrideId,
        mode: RestoreMode,
    ) -> Result<(), EngineError> {
        let (obj, prop) = self.redirect(obj, prop);
        let node = {
            let store = self.inner.store.borrow();
            store
                .objects
                .get(obj)
                .ok_or(EngineError::DeadObject(obj))?
                .node(prop)
        };
        let n = node.ok_or(EngineError::Reactive(
            ljos_reactive::ReactiveError::UnknownOverride(NodeId::null(), id),
        ))?;
        let g = &self.inner.graph;
        g.batch(|| {
            g.remove_override(n, id, mode)?;
            self.sync_field(obj, prop, n);
            Ok(())
        })
    }

    fn sync_field(&self, obj: ObjectId, prop: PropIndex, n: NodeId) {
        let g = &self.inner.graph;
        if let Some(v) = g.untrack(|| g.try_get(n)) {
            self.store_value(obj, prop, &v);
        }
    }

    /// Defers effects (handlers, subscribers) until `f` returns.
    pub fn batch<R>(&self, f: impl FnOnce() -> R) -> R {
        self.inner.graph.batch(f)
    }

    /// Runs `f` without tracking reads.
    pub fn untrack<R>(&self, f: impl FnOnce() -> R) -> R {
        let _t = Tracking::pause(&self.inner.tracking);
        self.inner.graph.untrack(f)
    }

    /// An eager effect: runs now, then whenever what it read changes. A
    /// Rust subscriber. Dispose the returned node to stop it.
    pub fn effect(&self, f: impl FnMut(&Engine) + 'static) -> Result<NodeId, EngineError> {
        self.effect_scoped(self.inner.graph.root(), f)
    }

    /// [`Engine::effect`] that dies with `obj`.
    pub fn effect_in(
        &self,
        obj: ObjectId,
        f: impl FnMut(&Engine) + 'static,
    ) -> Result<NodeId, EngineError> {
        let scope = self.ensure_scope(obj)?;
        self.effect_scoped(scope, f)
    }

    /// Calls `f(new value)` whenever `obj`'s `prop` changes (not now), until
    /// `obj` dies. Untracked, like a signal handler.
    pub fn watch(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        f: impl Fn(&Engine, &Value) + 'static,
    ) -> Result<NodeId, EngineError> {
        let n = self.ensure_node(obj, prop)?;
        let scope = self.ensure_scope(obj)?;
        let weak = Rc::downgrade(&self.inner);
        Ok(self.inner.graph.watch(scope, n, move |_g, v| {
            if let Some(inner) = weak.upgrade() {
                let engine = Engine::from_inner(inner);
                let _t = Tracking::pause(&engine.inner.tracking);
                f(&engine, v);
            }
        })?)
    }

    fn effect_scoped(
        &self,
        scope: ScopeId,
        mut f: impl FnMut(&Engine) + 'static,
    ) -> Result<NodeId, EngineError> {
        let weak = Rc::downgrade(&self.inner);
        let g = &self.inner.graph;
        Ok(g.effect(scope, move |_g| {
            if let Some(inner) = weak.upgrade() {
                let engine = Engine::from_inner(inner);
                let _t = Tracking::enter(&engine.inner.tracking);
                f(&engine);
            }
        })?)
    }

    // ------------------------------------------------------------- methods

    pub fn method_index(&self, obj: ObjectId, name: &str) -> Result<MethodIndex, EngineError> {
        let store = self.inner.store.borrow();
        let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
        h.meta
            .method_index(name)
            .ok_or_else(|| EngineError::UnknownMethod {
                ty: h.meta.name.to_string(),
                name: name.into(),
            })
    }

    /// Calls a native method. Arguments are coerced by the thunk.
    pub fn invoke(
        &self,
        obj: ObjectId,
        method: MethodIndex,
        args: &[Value],
    ) -> Result<Value, EngineError> {
        let thunk = {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
            let m = h
                .meta
                .method(method)
                .ok_or_else(|| EngineError::UnknownMethod {
                    ty: h.meta.name.to_string(),
                    name: format!("#{}", method.0),
                })?;
            match m.thunk {
                Some(t) => Ok(t),
                None => Err(h
                    .ext
                    .as_ref()
                    .and_then(|e| e.methods.iter().find(|(i, _)| *i == method))
                    .map(|(_, f)| f.clone())
                    .ok_or_else(|| EngineError::NotNative(m.name.to_string()))?),
            }
        };
        let thunk = match thunk {
            Ok(t) => t,
            // A function declared in QML.
            Err(f) => return f(self, obj, args),
        };
        thunk(&Cx::new(self, obj), args)
    }

    /// Gives the QML-declared method `method` of `obj` its body.
    pub fn set_script_method(
        &self,
        obj: ObjectId,
        method: MethodIndex,
        f: crate::object::ScriptMethod,
    ) -> Result<(), EngineError> {
        let mut store = self.inner.store.borrow_mut();
        let h = store
            .objects
            .get_mut(obj)
            .ok_or(EngineError::DeadObject(obj))?;
        let ext = h.ext_mut();
        match ext.methods.iter_mut().find(|(i, _)| *i == method) {
            Some(e) => e.1 = f,
            None => ext.methods.push((method, f)),
        }
        Ok(())
    }

    pub fn invoke_by_name(
        &self,
        obj: ObjectId,
        name: &str,
        args: &[Value],
    ) -> Result<Value, EngineError> {
        let m = self.method_index(obj, name)?;
        self.invoke(obj, m, args)
    }

    /// Runs the native `on_complete` hooks, base type first, once.
    pub fn complete(&self, obj: ObjectId) -> Result<(), EngineError> {
        let hooks: SmallVec<[CompleteThunk; 4]> = {
            let mut store = self.inner.store.borrow_mut();
            let h = store
                .objects
                .get_mut(obj)
                .ok_or(EngineError::DeadObject(obj))?;
            if h.flags.contains(ObjectFlags::COMPLETED) {
                return Ok(());
            }
            h.flags |= ObjectFlags::COMPLETED;
            let mut hooks: SmallVec<_> = h.meta.chain().filter_map(|m| m.on_complete).collect();
            hooks.reverse();
            hooks
        };
        let cx = Cx::new(self, obj);
        for f in hooks {
            f(&cx)?;
        }
        Ok(())
    }

    /// The object `attacher` attaches to `obj` (`Keys` in `Keys.onPressed`),
    /// created and owned by `obj` on first use.
    pub fn attached_object(
        &self,
        obj: ObjectId,
        attacher: &MetaRef,
    ) -> Result<ObjectId, EngineError> {
        let make = attacher
            .attached
            .ok_or_else(|| EngineError::NoAttachedType(attacher.name.to_string()))?;
        let key = attacher.type_id();
        {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
            if let Some((_, a)) = h.attached.iter().find(|(k, _)| *k == key) {
                return Ok(*a);
            }
        }
        let a = self.create(make(), Some(obj))?;
        self.inner.store.borrow_mut().objects[obj]
            .attached
            .push((key, a));
        Ok(a)
    }

    // ------------------------------------------------------------ contexts

    pub fn root_context(&self) -> ContextId {
        self.inner.root_context
    }

    /// A child context (one per component instance).
    pub fn create_context(&self, parent: Option<ContextId>) -> ContextId {
        self.inner.contexts.borrow_mut().insert(Context {
            parent: Some(parent.unwrap_or(self.inner.root_context)),
            ..Context::default()
        })
    }

    pub fn destroy_context(&self, ctx: ContextId) {
        if ctx != self.inner.root_context {
            self.inner.contexts.borrow_mut().remove(ctx);
        }
    }

    /// Declares `id: name` in `ctx`.
    pub fn set_context_id(&self, ctx: ContextId, name: &str, obj: ObjectId) {
        if let Some(c) = self.inner.contexts.borrow_mut().get_mut(ctx) {
            c.ids.insert(Atom::intern(name), obj);
        }
    }

    /// Resolves an id from `ctx` outwards. An id whose object is gone
    /// resolves to `None` (`null` in QML).
    pub fn resolve_id(&self, ctx: ContextId, name: &str) -> Option<ObjectId> {
        let found = {
            let contexts = self.inner.contexts.borrow();
            let mut cur = Some(ctx);
            let mut found = None;
            while let Some(c) = cur.and_then(|c| contexts.get(c)) {
                if let Some(o) = c.ids.get(name) {
                    found = Some(*o);
                    break;
                }
                cur = c.parent;
            }
            found
        };
        found.filter(|o| self.is_alive(*o))
    }

    pub fn set_context_object(&self, ctx: ContextId, obj: Option<ObjectId>) {
        if let Some(c) = self.inner.contexts.borrow_mut().get_mut(ctx) {
            c.context_object = obj;
        }
    }

    /// Puts `name` in scope for everything created in `ctx` and its
    /// children (`setContextProperty`).
    pub fn set_context_property(&self, ctx: ContextId, name: &str, v: Value) {
        if let Some(c) = self.inner.contexts.borrow_mut().get_mut(ctx) {
            c.props.insert(Atom::intern(name), v);
        }
    }

    /// A context property, looked up from `ctx` outwards.
    pub fn context_property(&self, ctx: ContextId, name: &str) -> Option<Value> {
        let contexts = self.inner.contexts.borrow();
        let mut cur = Some(ctx);
        while let Some(c) = cur.and_then(|c| contexts.get(c)) {
            if let Some(v) = c.props.get(name) {
                return Some(v.clone());
            }
            cur = c.parent;
        }
        None
    }

    pub fn context_object(&self, ctx: ContextId) -> Option<ObjectId> {
        self.inner.contexts.borrow().get(ctx)?.context_object
    }

    pub fn context_parent(&self, ctx: ContextId) -> Option<ContextId> {
        self.inner.contexts.borrow().get(ctx)?.parent
    }

    pub fn context_of(&self, obj: ObjectId) -> Option<ContextId> {
        Some(self.inner.store.borrow().objects.get(obj)?.context)
    }

    pub fn set_context(&self, obj: ObjectId, ctx: ContextId) -> Result<(), EngineError> {
        let mut store = self.inner.store.borrow_mut();
        store
            .objects
            .get_mut(obj)
            .ok_or(EngineError::DeadObject(obj))?
            .context = ctx;
        Ok(())
    }

    // -------------------------------------------------------- translation

    /// Re-runs every binding that produced a translated string (`qsTr`),
    /// after the host changed the language.
    pub fn retranslate(&self) {
        let n = self.translation_node();
        let g = &self.inner.graph;
        let next = match g.untrack(|| g.try_get(n)) {
            Some(Value::Int(i)) => i.wrapping_add(1),
            _ => 1,
        };
        let _ = g.write(n, Value::Int(next), WriteKind::Initial);
    }

    /// Makes the running binding depend on the language.
    pub fn track_translation(&self) {
        let n = self.translation_node();
        let _ = self.inner.graph.try_get(n);
    }

    fn translation_node(&self) -> NodeId {
        if let Some(n) = self.inner.translation.get() {
            return n;
        }
        let g = &self.inner.graph;
        let n = g
            .property(g.root(), Value::Int(0))
            .expect("the root scope is always alive");
        self.inner.translation.set(Some(n));
        n
    }

    // ------------------------------------------------------------- strings

    /// `ToString` with object names: `Rectangle(0x1)`, as QML prints
    /// objects (with the QML type name rather than the C++ class).
    pub fn to_js_string(&self, v: &Value) -> Atom {
        match v {
            Value::Object(id) => match self.meta_of(*id) {
                Some(m) => Atom::from(format!("{}(0x{:x})", m.name, id.data().as_ffi())),
                None => Atom::intern("null"),
            },
            v => v.to_js_string(),
        }
    }

    /// The object's scope in the graph, if it has nodes yet.
    pub fn object_scope(&self, obj: ObjectId) -> Option<ScopeId> {
        self.inner.store.borrow().objects.get(obj)?.scope
    }
}

fn not_in(order: &[ObjectId], id: ObjectId) -> bool {
    !order.contains(&id)
}

fn unknown_index(meta: &MetaObject, prop: PropIndex) -> EngineError {
    EngineError::UnknownProperty {
        ty: meta.name.to_string(),
        name: format!("#{}", prop.0),
    }
}

/// Coerces for a property of type `ty`, checking object classes.
pub(crate) fn coerce_in(store: &Store, ty: &TypeTag, v: &Value) -> Result<Value, EngineError> {
    let c = ty.coerce(v)?;
    if let (TypeTag::Object(Some(name)), Value::Object(id)) = (ty, &c) {
        match store.objects.get(*id) {
            None => return Ok(Value::Null),
            Some(h) if !h.meta.inherits_name(name) => {
                return Err(ConvertError {
                    from: h.meta.name.to_string(),
                    to: name.to_string(),
                }
                .into());
            }
            Some(_) => {}
        }
    }
    Ok(c)
}

fn read_access(native: &dyn QmlObject, dynamic: &[Value], access: &PropertyAccess) -> Value {
    match *access {
        PropertyAccess::Field { owner, get, .. } => {
            find_native(native, owner).map_or(Value::Undefined, get)
        }
        PropertyAccess::Group {
            owner,
            get,
            meta,
            leaf,
            ..
        } => match find_native(native, owner) {
            Some(o) => read_access(get(o), &[], &meta.properties[leaf.0 as usize].access),
            None => Value::Undefined,
        },
        PropertyAccess::GroupRoot => Value::Undefined,
        PropertyAccess::Dynamic { slot } => dynamic.get(slot as usize).cloned().unwrap_or_default(),
    }
}

fn write_access(
    native: &mut dyn QmlObject,
    dynamic: &mut [Value],
    access: &PropertyAccess,
    v: &Value,
) -> Result<(), ConvertError> {
    match *access {
        PropertyAccess::Field { owner, set, .. } => match find_native_mut(native, owner) {
            Some(o) => set(o, v),
            None => Ok(()),
        },
        PropertyAccess::Group {
            owner,
            get_mut,
            meta,
            leaf,
            ..
        } => match find_native_mut(native, owner) {
            Some(o) => write_access(
                get_mut(o),
                &mut [],
                &meta.properties[leaf.0 as usize].access,
                v,
            ),
            None => Ok(()),
        },
        PropertyAccess::GroupRoot => Ok(()),
        PropertyAccess::Dynamic { slot } => {
            if let Some(s) = dynamic.get_mut(slot as usize) {
                *s = v.clone();
            }
            Ok(())
        }
    }
}

fn store_in(h: &mut ObjectHeader, prop: PropIndex, v: &Value) -> Result<(), ConvertError> {
    let Some(access) = h.meta.property(prop).map(|p| p.access) else {
        return Ok(());
    };
    write_access(&mut *h.native, &mut h.dynamic, &access, v)
}
