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
use std::time::Instant;

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
    AliasTarget, FieldReads, Location, ObjectFlags, ObjectHeader, QmlObject, QmlType,
    downcast_chain, downcast_chain_mut, find_native, find_native_mut,
};
use crate::registry::{ModuleDef, Registry, TypeDump, Version};
use crate::signal::Signals;
use crate::types::{ConvertError, QmlValue, TypeTag};
use crate::value::Value;

/// Called when an object's visual parent changes: (object, new parent).
/// A binding's evaluator; `None` keeps the old value (it threw).
pub(crate) type BindingFn = Box<dyn FnMut(&Engine) -> Option<Value>>;

/// Which values of a binding no interceptor sees.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Free {
    Never,
    /// The first: the value the property is created with.
    First,
    Always,
}

pub type ParentListener = Rc<dyn Fn(&Engine, ObjectId, Option<ObjectId>)>;

/// Called when an attached object is first made: (object, attached object).
/// Returns whether it wants to be called again.
pub(crate) type AttachedListener = Rc<dyn Fn(&Engine, ObjectId, ObjectId) -> bool>;

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
    /// Evaluate bindings at creation instead of on first read.
    pub(crate) eager_bindings: Cell<bool>,
    /// While a creation applies its objects in eager mode: the bindings
    /// installed so far. They first run when the creation is whole, in
    /// Qt's order, as a binding does not run in Qt before then either.
    pub(crate) eager_hold: RefCell<Option<Vec<Held>>>,
    /// Read by every translated string; bumped by
    /// [`Engine::retranslate`].
    pub(crate) translation: Cell<Option<NodeId>>,
    pub(crate) parent_listeners: RefCell<Vec<ParentListener>>,
    pub(crate) attached_listeners: RefCell<Vec<AttachedListener>>,
    /// Set by the first `Component.onDestruction` handler, so destroying
    /// objects costs nothing extra in an engine that has none.
    pub(crate) destruction_handlers: Cell<bool>,
    context_serial: Cell<u64>,
    /// Interceptors, value sources and per-engine extensions.
    pub(crate) hooks: crate::hooks::Hooks,
    /// Dev hot reload: empty and untouched unless tracking is turned on.
    pub(crate) reload: crate::incubator::reload::ReloadState,
    /// Tier 1 (bindings as bytecode): its switches, and what its
    /// cross-check keeps while it runs.
    pub(crate) tier1: crate::incubator::tier1::State,
    /// Faults on their way to a boundary, and the boundaries (fault.rs,
    /// boundary.rs).
    pub(crate) faults: crate::fault::FaultState,
    /// The slow native call watchdog (slow.rs): its budget and the calls
    /// that ran over it. Untouched beyond one branch while off.
    pub(crate) slow: crate::slow::SlowState,
    /// What bindings say while their objects are made (delayed_errors.rs).
    pub(crate) delayed: crate::delayed_errors::DelayedErrors,
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

/// The native properties that are bindable in Qt (type, property), found
/// by running qml6 6.11.2. Qt reports a binding loop on one of them in its
/// one-line form and first evaluates their bindings after all others.
/// Only diagnostics and the eager order depend on it, so the list lives
/// here rather than as a flag on every property.
const QT_BINDABLE: &[(&str, &str)] = &[
    ("QtObject", "objectName"),
    ("Item", "x"),
    ("Item", "y"),
    ("Item", "width"),
    ("Item", "height"),
];

/// Whether `prop` of a `meta` object is one of [`QT_BINDABLE`].
fn qt_bindable(meta: &MetaObject, prop: PropIndex) -> bool {
    let Some(p) = meta.property(prop) else {
        return false;
    };
    matches!(p.access, PropertyAccess::Field { .. })
        && QT_BINDABLE.iter().any(|(native, name)| {
            *p.name == **name
                && meta
                    .chain()
                    .any(|m| m.native.is_some() && *m.name == **native)
        })
}

/// A binding installed while a creation holds eager evaluation back.
pub(crate) struct Held {
    pub(crate) node: NodeId,
    pub(crate) obj: ObjectId,
    pub(crate) prop: PropIndex,
    /// The instance it was written in and the offset of its name there;
    /// `None` for a binding a native type installs.
    pub(crate) place: Option<(usize, u32)>,
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
            eager_bindings: Cell::new(false),
            eager_hold: RefCell::new(None),
            translation: Cell::new(None),
            parent_listeners: RefCell::new(Vec::new()),
            attached_listeners: RefCell::new(Vec::new()),
            destruction_handlers: Cell::new(false),
            context_serial: Cell::new(0),
            hooks: crate::hooks::Hooks::default(),
            reload: Default::default(),
            tier1: Default::default(),
            faults: Default::default(),
            slow: Default::default(),
            delayed: Default::default(),
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
        let mut dump = self.inner.registry.borrow().dump();
        for module in &mut dump.modules {
            for ty in &mut module.types {
                ty.singleton = self.is_native_singleton(&module.uri, &ty.name);
            }
        }
        dump
    }

    // ------------------------------------------------------------ warnings

    fn report(&self, d: &Diagnostic) {
        let node = match d {
            Diagnostic::BindingLoop { node, .. } | Diagnostic::RerunLimit { node, .. } => *node,
        };
        let key = self.inner.store.borrow().node_keys.get(node).copied();
        let what = key.map(|k| self.describe(k.object, k.prop));
        let msg = match (d, what) {
            (Diagnostic::BindingLoop { .. }, Some(_)) => {
                let k = key.expect("described");
                self.loop_message(k.object, k.prop)
            }
            (Diagnostic::RerunLimit { runs, .. }, Some((ty, prop))) => {
                format!("QML {ty}: property \"{prop}\" handlers re-ran {runs} times in one update")
            }
            (d, None) => format!("QML: {d}"),
        };
        self.warn(msg);
    }

    /// Qt's binding-loop message for `prop` of `obj`. Qt has two forms:
    /// most properties name the object and give the binding's place on a
    /// second line; the few that are bindable in Qt ([`QT_BINDABLE`]) have
    /// one line at the binding's place.
    fn loop_message(&self, obj: ObjectId, prop: PropIndex) -> String {
        let store = self.inner.store.borrow();
        let Some(h) = store.objects.get(obj) else {
            return "QML ?: Binding loop detected".to_owned();
        };
        let info = h.meta.property(prop);
        let ext = h.ext.as_ref();
        let binding = ext.and_then(|e| e.binding_locs.iter().find(|l| l.prop == prop));
        let place = |l: &Location| format!("{}:{}:{}", l.url, l.line, l.column);
        let object_at = ext.and_then(|e| e.location.as_ref()).map(place);
        let binding_at = binding.map(|b| place(&b.at));

        let mut ty = h.meta.name.to_string();
        let mut name = info.map_or_else(|| "?".to_owned(), |p| p.name.to_string());
        let mut two_lines = true;
        match info.map(|p| &p.access) {
            Some(PropertyAccess::Field { .. }) => two_lines = !qt_bindable(&h.meta, prop),
            Some(PropertyAccess::Group { meta, leaf, .. }) => {
                ty = format!("{ty} (parent or ancestor of {})", meta.name);
                if let Some(p) = meta.property(*leaf) {
                    name = p.name.to_string();
                }
            }
            _ => {}
        }
        if let Some(member) = binding.and_then(|b| b.member.as_ref()) {
            two_lines = true;
            name = member.to_string();
        }
        let what = format!("QML {ty}: Binding loop detected for property \"{name}\"");
        match (two_lines, object_at, binding_at) {
            (true, Some(o), Some(b)) => format!("{o}: {what}:\n{b}"),
            (false, _, Some(b)) => format!("{b}: {what}"),
            // A binding made at run time (`Qt.binding`) has no place.
            (_, Some(o), None) => format!("{o}: {what}"),
            (_, None, Some(b)) => format!("{b}: {what}"),
            (_, None, None) => what,
        }
    }

    /// Where the binding of `prop` was written (`file.qml:4:9`), when it was
    /// written in a file.
    fn binding_place(&self, obj: ObjectId, prop: PropIndex) -> Option<String> {
        let store = self.inner.store.borrow();
        let ext = store.objects.get(obj)?.ext.as_ref()?;
        let l = &ext.binding_locs.iter().find(|l| l.prop == prop)?.at;
        Some(format!("{}:{}:{}", l.url, l.line, l.column))
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
    /// The object is not complete: its `on_complete` hooks wait for
    /// [`Engine::complete`], as they do for an object a component is still
    /// building.
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

    /// Makes an object of a registered type, complete: its `on_complete`
    /// hooks have run, so a `Timer` set running afterwards runs and a
    /// `Connections` given a target connects. Calling
    /// [`Engine::complete`] on it again does nothing.
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
        let obj = self.create(meta, owner)?;
        self.complete_made(obj)
    }

    /// Adds a native value built in Rust, complete: its `on_complete`
    /// hooks have run with the fields it was given, so
    /// `Timer { running: true, ..Default::default() }` runs from here on,
    /// as a running `Timer` does once its component is complete.
    pub fn create_native<T: QmlType>(
        &self,
        native: T,
        owner: Option<ObjectId>,
    ) -> Result<ObjectId, EngineError> {
        let obj = self.create_native_incomplete(native, owner)?;
        self.complete_made(obj)
    }

    /// Adds a native value built in Rust without completing it, for code
    /// that sets more on it (properties, children, a parent) before its
    /// `on_complete` hooks may look: it calls [`Engine::complete`] when it
    /// is done. Until then a `Timer` does not run and a `Connections`
    /// does not connect.
    pub fn create_native_incomplete<T: QmlType>(
        &self,
        native: T,
        owner: Option<ObjectId>,
    ) -> Result<ObjectId, EngineError> {
        self.create_with(MetaRef::Static(T::static_meta()), Box::new(native), owner)
    }

    /// Completes what was just made; an object whose hooks fail is not
    /// handed out half set up.
    fn complete_made(&self, obj: ObjectId) -> Result<ObjectId, EngineError> {
        match self.complete(obj) {
            Ok(()) => Ok(obj),
            Err(e) => {
                self.destroy_now(obj);
                Err(e)
            }
        }
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
                    .ok_or_else(|| EngineError::dead(o))?
                    .context
            }
            None => self.inner.root_context,
        };
        let mut native = native;
        if let Some(t) = downcast_chain_mut::<crate::qtqml::Timer>(&mut *native) {
            t.parent = owner;
        }
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

    /// Gives `obj` the type `meta`, which must derive from the type it
    /// has: for an object whose properties are only known at run time
    /// (the names the `PathAttribute`s of a path give `PathView`'s
    /// attached object). What it had keeps its value.
    pub fn extend_type(&self, obj: ObjectId, meta: MetaRef) -> Result<(), EngineError> {
        self.extend_meta(obj, meta)
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

    /// Runs the `destroyed` handlers of `obj` and what it owns, owners
    /// first.
    fn tell_destroyed(&self, obj: ObjectId) {
        let handlers = {
            let store = self.inner.store.borrow();
            let mut order = Vec::new();
            let mut stack = vec![obj];
            while let Some(id) = stack.pop() {
                if let Some(h) = store.objects.get(id) {
                    order.push(id);
                    stack.extend(h.children.iter().rev().copied());
                }
            }
            drop(store);
            self.inner.signals.borrow_mut().take_destroyed(&order)
        };
        for (id, handler) in handlers {
            let arg = [Value::Object(id)];
            // A panic ends this handler only (fault.rs).
            self.guarded(id, || handler(self, &arg));
        }
    }

    /// Destroys `obj` and everything it owns, children first. Its reactive
    /// nodes are disposed, so bindings that read it re-run and see `null`;
    /// its connections, as sender or receiver, are dropped.
    /// `Component.onDestruction` handlers run first, while everything is
    /// still alive ([`Engine::announce_destruction`]), and then the
    /// handlers connected to `destroyed` ([`SignalIndex::DESTROYED`](crate::SignalIndex::DESTROYED)):
    /// an owner's before what it owns, as in Qt, where an object says
    /// it is destroyed before it deletes its children. Each is handed
    /// the object, still alive, and runs once.
    pub fn destroy_now(&self, obj: ObjectId) -> bool {
        self.tell_destruction(obj, false);
        self.tell_destroyed(obj);
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
        // Survivors that may still list the dead: the owner and the
        // visual parent of the object destroyed.
        let mut holders: SmallVec<[ObjectId; 2]> = SmallVec::new();
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
            if let Some(h) = headers.last() {
                for p in [h.owner, h.visual_parent].into_iter().flatten() {
                    if s.objects.contains_key(p) && !holders.contains(&p) {
                        holders.push(p);
                    }
                }
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
        self.inner.hooks.forget(&order);
        if self.inner.reload.tracking.get() {
            self.reload_forget(&order);
        }
        let js = self.inner.js.borrow().clone();
        if let Some(js) = js {
            js.forget(&order);
        }
        self.inner.graph.batch(|| {
            for h in &headers {
                if let Some(scope) = h.scope {
                    let _ = self.inner.graph.dispose_scope(scope);
                }
                // Read without nodes, the properties go as if they had.
                if let Some(r) = &h.field_reads {
                    self.inner.graph.readers_lost(r.all());
                }
            }
            for c in &orphans {
                self.parent_changed(*c, None);
            }
            for p in &holders {
                self.forget_in_lists(*p, &order);
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

    /// Takes `dead` out of the native list properties of `holder`
    /// (`Item.children`, `resources`, `data`), as Qt's own lists drop a
    /// deleted object. A `property list<T>` written in QML keeps its
    /// length and reads `null` there, so it is left alone.
    fn forget_in_lists(&self, holder: ObjectId, dead: &[ObjectId]) {
        let lists: SmallVec<[PropIndex; 4]> = {
            let store = self.inner.store.borrow();
            let Some(h) = store.objects.get(holder) else {
                return;
            };
            h.meta
                .properties
                .iter()
                .enumerate()
                .filter(|(_, p)| p.is_list() && matches!(p.access, PropertyAccess::Field { .. }))
                .map(|(i, _)| PropIndex(i as u32))
                .collect()
        };
        for p in lists {
            let Value::List(l) = self.untrack(|| self.read(holder, p)) else {
                continue;
            };
            let gone = |v: &Value| v.as_object().is_some_and(|o| dead.contains(&o));
            if !l.iter().any(gone) {
                continue;
            }
            let kept = l.iter().filter(|v| !gone(v)).cloned().collect();
            let _ = self.write_internal(holder, p, Value::list(kept), WriteKind::Initial);
        }
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
                let h = store.objects.get(c).ok_or_else(|| EngineError::dead(c))?;
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
        let scope = store.objects[obj].scope;
        let timer = downcast_chain_mut::<crate::qtqml::Timer>(&mut *store.objects[obj].native)
            .map(|t| t.parent = owner)
            .is_some();
        drop(store);
        if timer {
            self.notify(obj, crate::qtqml::Timer::PARENT.index)?;
        }
        // The object's nodes and effects die with its new owner from now
        // on, as its native part does.
        if let Some(s) = scope {
            let g = &self.inner.graph;
            let parent = match owner {
                Some(o) => self.ensure_scope(o)?,
                None => g.root(),
            };
            g.reparent_scope(s, parent)?;
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
            let h = store
                .objects
                .get(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
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
            let h = store
                .objects
                .get(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
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
        let h = store
            .objects
            .get(obj)
            .ok_or_else(|| EngineError::dead(obj))?;
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
            .ok_or_else(|| EngineError::dead(obj))?;
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
        let h = store
            .objects
            .get(obj)
            .ok_or_else(|| EngineError::dead(obj))?;
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
            /// Tracked, without a node: the read is kept on the object.
            Field(Value),
            Alias(Option<AliasTarget>),
        }
        self.make_if_due(obj, prop);
        let got = {
            let store = self.inner.store.borrow();
            // Every binding's read comes here: the error is built only
            // when the object is gone.
            let Some(h) = store.objects.get(obj) else {
                return Err(EngineError::DeadObject(obj));
            };
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
                    None => Got::Field(read_access(&*h.native, &h.dynamic, &p.access)),
                }
            }
        };
        let v = match got {
            // A record a native field holds is converted on each read; held
            // in a node, it is one value, and so one script object
            // (records.rs), until the field is set or notified again.
            Got::Value(Value::Map(_)) if self.is_native_field(obj, prop) => {
                let n = self.ensure_node(obj, prop)?;
                self.inner.graph.try_get(n).unwrap_or_default()
            }
            Got::Value(v) => v,
            Got::Node(n) => self.inner.graph.try_get(n).unwrap_or_default(),
            Got::Field(v) => {
                self.keep_field_read(obj, prop);
                v
            }
            Got::Alias(Some(AliasTarget {
                object,
                prop: Some(p),
                member: None,
            })) => return self.try_read(object, p),
            Got::Alias(Some(AliasTarget {
                object,
                prop: Some(p),
                member: Some(m),
            })) => {
                let whole = self.try_read(object, p)?;
                crate::incubator::tier1::member_of(&whole, &m).unwrap_or_default()
            }
            Got::Alias(Some(AliasTarget {
                object, prop: None, ..
            })) => Value::Object(object),
            // Not wired yet (still being created).
            Got::Alias(None) => Value::Undefined,
        };
        Ok(self.live(v))
    }

    fn is_native_field(&self, obj: ObjectId, prop: PropIndex) -> bool {
        let store = self.inner.store.borrow();
        store
            .objects
            .get(obj)
            .and_then(|h| h.meta.property(prop))
            .is_some_and(|p| {
                matches!(p.access, PropertyAccess::Field { .. })
                    && !p.flags.contains(PropertyFlags::CONSTANT)
            })
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
        self.set_alias_to(obj, prop, target, target_prop, None)
    }

    /// Points the alias property `prop` of `obj` at the member `member` of
    /// `target`'s property `target_prop` (`label.font.family`).
    pub(crate) fn set_member_alias(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        target: ObjectId,
        target_prop: PropIndex,
        member: &str,
    ) -> Result<(), EngineError> {
        self.set_alias_to(
            obj,
            prop,
            target,
            Some(target_prop),
            Some(Atom::new(member)),
        )
    }

    fn set_alias_to(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        target: ObjectId,
        target_prop: Option<PropIndex>,
        member: Option<Atom>,
    ) -> Result<(), EngineError> {
        let mut store = self.inner.store.borrow_mut();
        let h = store
            .objects
            .get_mut(obj)
            .ok_or_else(|| EngineError::dead(obj))?;
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
            member,
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

    /// The group `prop` of `obj` is, or the group it leads to as an alias
    /// of a whole group (`frame: box.border`): its object and its name.
    pub(crate) fn group_at(&self, obj: ObjectId, prop: PropIndex) -> Option<(ObjectId, Atom)> {
        let is_group = |info: &crate::PropertyInfo| info.flags.contains(PropertyFlags::GROUP);
        let alias = {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj)?;
            let info = h.meta.property(prop)?;
            if !info.is_alias() {
                return is_group(info).then(|| (obj, info.name.clone()));
            }
            h.alias(prop)?
        };
        if alias.prop.is_none() || alias.member.is_some() {
            return None;
        }
        let (obj, prop) = self.redirect(obj, prop);
        let store = self.inner.store.borrow();
        let info = store.objects.get(obj)?.meta.property(prop)?;
        is_group(info).then(|| (obj, info.name.clone()))
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
                        ..
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
        self.write(obj, p, v, WriteKind::Imperative)?;
        if self.inner.reload.tracking.get() {
            // An assignment from outside the file is the user's, which a
            // dev reload carries across.
            self.reload_note_assigned(obj, p);
        }
        Ok(())
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

    /// Writes a value the property did not get from its author: one that
    /// follows something else (a control's font and palette follow its
    /// ancestors'). Readers see it and a binding stays in place, as with
    /// an animation step, and the property does not read as assigned:
    /// [`Engine::provenance`] says `Default` or `Animated` afterwards,
    /// unless it was assigned before.
    pub fn write_derived(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        v: Value,
    ) -> Result<(), EngineError> {
        let (obj, prop) = self.redirect(obj, prop);
        self.drop_maker(obj, prop);
        let v = self.for_slot(obj, prop, v);
        {
            let mut store = self.inner.store.borrow_mut();
            let h = store
                .objects
                .get_mut(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            if h.node(prop).is_none() && !h.read_without_node(prop) {
                // Nobody observes it: the field is the only copy, and it
                // is not marked assigned.
                if let Some(v) = filter_in(h, prop, v)? {
                    store_in(h, prop, &v)?;
                }
                return Ok(());
            }
        }
        self.write_impl(obj, prop, v, WriteKind::Animation, true)
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

    pub(crate) fn write_impl(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        v: Value,
        how: WriteKind,
        internal: bool,
    ) -> Result<(), EngineError> {
        if let Some(target) = self.alias_member(obj, prop) {
            return self.write_member(target, &v, how, internal);
        }
        let (obj, prop) = self.redirect(obj, prop);
        self.drop_maker(obj, prop);
        // `x = undefined` takes a property with a reset back, and so does
        // a value that stands for none.
        if matches!(how, WriteKind::Imperative | WriteKind::Initial)
            && self.takes_back(obj, prop, &v)
        {
            return self.reset_impl(obj, prop, internal);
        }
        // An array is the script's own in a `var`, a list anywhere else
        // (script_arrays.rs).
        let v = self.for_slot(obj, prop, v);
        let (v, node, hook) = {
            let store = self.inner.store.borrow();
            let h = store
                .objects
                .get(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            let ty = self.check_writable(h, prop, internal)?;
            let hook = match &h.ext {
                Some(ext) if !internal => ext.write_hook.clone(),
                _ => None,
            };
            let node = match h.node(prop) {
                None if h.read_without_node(prop) => Err(()),
                n => Ok(n),
            };
            let v = coerce_for(&store, Some(obj), &ty, &v)?;
            // The type's own say comes before anything hears of it, as a
            // setter's does in Qt.
            let Some(v) = filter_in(h, prop, v)? else {
                return Ok(());
            };
            (v, node, hook)
        };
        let node = match node {
            Ok(n) => n,
            // A reader follows it: the node, made with the old value,
            // tells it of the new one.
            Err(()) => Some(self.ensure_node(obj, prop)?),
        };
        // A model row: the write goes to the model, which writes back.
        if let Some(hook) = hook
            && hook(self, prop, &v)
        {
            return Ok(());
        }
        // A Behavior: the value goes to it, and it moves the property.
        if !internal && matches!(how, WriteKind::Imperative) && self.intercept_write(obj, prop, &v)
        {
            return Ok(());
        }
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
                    .ok_or_else(|| EngineError::dead(obj))?;
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
        let h = store
            .objects
            .get(obj)
            .ok_or_else(|| EngineError::dead(obj))?;
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
            let h = store
                .objects
                .get(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
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
        // The node carries the provenance over. A write would do that
        // too, but a write flushes: this runs in the middle of whichever
        // binding read the property first, and the flush would run other
        // queued bindings against that binding's half-made value.
        let provenance = if assigned {
            Provenance::Assigned
        } else {
            Provenance::Default
        };
        let n = g.property_with(scope, value, provenance)?;
        let readers = {
            let mut store = self.inner.store.borrow_mut();
            store.node_keys.insert(n, PropKey { object: obj, prop });
            let h = &mut store.objects[obj];
            h.set_node(prop, n);
            h.field_reads.as_mut().map(|r| r.take(prop))
        };
        // Who read the field without a node reads the node now.
        if let Some(readers) = readers {
            g.adopt_readers(n, readers);
        }
        Ok(n)
    }

    /// For tools and tests: the graph node of a property, made if it has
    /// none. A tracked read makes none for a property that never changes;
    /// the node made here is read by whoever read the property.
    pub fn property_node(&self, obj: ObjectId, prop: PropIndex) -> Result<NodeId, EngineError> {
        self.ensure_node(obj, prop)
    }

    /// Keeps a tracked read of a property that has no node
    /// ([`FieldReads`]).
    fn keep_field_read(&self, obj: ObjectId, prop: PropIndex) {
        let g = &self.inner.graph;
        let Some(r) = g.reader() else {
            return;
        };
        let mut store = self.inner.store.borrow_mut();
        if let Some(h) = store.objects.get_mut(obj) {
            let len = h.meta.properties.len();
            h.field_reads
                .get_or_insert_with(|| Box::new(FieldReads::new(len)))
                .add(prop, r, |e| g.is_current(e));
        }
    }

    /// The node of a property something observes, made if only a read
    /// without one did: what a change has to go through to be seen.
    pub(crate) fn observed_node(&self, obj: ObjectId, prop: PropIndex) -> Option<NodeId> {
        let read = {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj)?;
            if let Some(n) = h.node(prop) {
                return Some(n);
            }
            h.read_without_node(prop)
        };
        if read {
            self.ensure_node(obj, prop).ok()
        } else {
            None
        }
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

    /// [`Engine::bind`] for a value native code computes (a size that
    /// follows the implicit size): nobody wrote it, so no interceptor sees
    /// what it gives, the first time or later. In Qt such a value is set
    /// from C++, which a `Behavior` does not animate either.
    pub fn bind_computed(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        f: impl FnMut(&Engine) -> Value + 'static,
    ) -> Result<(), EngineError> {
        let mut f = f;
        self.bind_free(
            obj,
            prop,
            Box::new(move |e| Some(f(e))),
            false,
            Free::Always,
        )
    }

    fn bind_impl(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        f: BindingFn,
        internal: bool,
    ) -> Result<(), EngineError> {
        // A binding the creator installs gives the property its first
        // value, which no interceptor animates.
        let free = if internal { Free::First } else { Free::Never };
        self.bind_free(obj, prop, f, internal, free)
    }

    pub(crate) fn bind_free(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        f: BindingFn,
        internal: bool,
        free: Free,
    ) -> Result<(), EngineError> {
        if let Some(target) = self.alias_member(obj, prop) {
            return self.bind_member(target, f, internal, free);
        }
        let (obj, prop) = self.redirect(obj, prop);
        self.drop_maker(obj, prop);
        let ty = {
            let store = self.inner.store.borrow();
            let h = store
                .objects
                .get(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            self.check_writable(h, prop, internal)?
        };
        let n = self.ensure_node(obj, prop)?;
        let g = &self.inner.graph;
        // What a failing binding keeps. The creator may be replacing a
        // binding that never ran (the one of the component it uses), and
        // Qt does not run that one either: reading it here could even
        // report a loop through a binding that is about to go.
        let last = if internal {
            g.peek(n)
        } else {
            g.untrack(|| g.try_get(n))
        };
        let wrapper = self.binding_wrapper(obj, prop, ty, last.unwrap_or_default(), f, free);
        g.bind(n, wrapper)?;
        if self.inner.eager_bindings.get() {
            let held = match self.inner.eager_hold.borrow_mut().as_mut() {
                Some(held) => {
                    held.push(Held {
                        node: n,
                        obj,
                        prop,
                        place: None,
                    });
                    true
                }
                None => false,
            };
            if !held {
                g.set_eager(n, true)?;
            }
        }
        Ok(())
    }

    /// Whether Qt evaluates the first binding of `prop` after all others
    /// (see [`QT_BINDABLE`]).
    pub(crate) fn is_qt_bindable(&self, obj: ObjectId, prop: PropIndex) -> bool {
        let store = self.inner.store.borrow();
        store
            .objects
            .get(obj)
            .is_some_and(|h| qt_bindable(&h.meta, prop))
    }

    /// Evaluates bindings when they are installed (those of a component
    /// being created: once it is whole, in Qt's order) and after every
    /// change, instead of when first read. Off by default; tests that
    /// expect Qt's eager behaviour (binding-loop warnings at creation)
    /// turn it on.
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
        mut free: Free,
    ) -> impl FnMut(&Graph<Value>) -> Value + 'static {
        let weak = Rc::downgrade(&self.inner);
        let reset = self.reset_of(obj, prop);
        move |_g| {
            let Some(inner) = weak.upgrade() else {
                return last.clone();
            };
            let engine = Engine::from_inner(inner);
            if engine.destruction_told(obj) {
                return last.clone();
            }
            let unseen = free != Free::Never;
            if free == Free::First {
                free = Free::Never;
            }
            let v = {
                let _t = Tracking::enter(&engine.inner.tracking);
                f(&engine)
            };
            let Some(v) = v else {
                return last.clone();
            };
            // A binding that yields `undefined` takes the property back
            // and stays: its next value is given as any other.
            let v = match reset {
                Some(reset) if reset.takes_back(&v) => {
                    let _t = Tracking::enter(&engine.inner.tracking);
                    engine.reset_value(obj, prop, reset)
                }
                _ => v,
            };
            let v = engine.for_slot(obj, prop, v);
            let coerced = {
                let store = engine.inner.store.borrow();
                coerce_for(&store, Some(obj), &ty, &v).and_then(|v| match store.objects.get(obj) {
                    Some(h) => Ok(filter_in(h, prop, v)?),
                    None => Ok(Some(v)),
                })
            };
            match coerced {
                // The type refused it: the property keeps what it holds.
                Ok(None) => {}
                Ok(Some(v)) => last = v,
                Err(e) => {
                    // Qt's message is the binding's place and the error; a
                    // binding made at run time has no place to give.
                    match engine.binding_place(obj, prop) {
                        Some(at) => engine.warn(format!("{at}: {e}")),
                        None => {
                            let (t, p) = engine.describe(obj, prop);
                            engine.warn(format!("QML {t}: {e} (property \"{p}\")"));
                        }
                    }
                }
            }
            if !unseen && let Some(shown) = engine.intercept_binding(obj, prop, &last) {
                return shown;
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
        let (n, cur) = {
            let store = self.inner.store.borrow();
            let h = store
                .objects
                .get(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            let p = h
                .meta
                .property(prop)
                .ok_or_else(|| unknown_index(&h.meta, prop))?;
            match h.node(prop) {
                Some(n) => (n, read_access(&*h.native, &h.dynamic, &p.access)),
                // Nothing observes it.
                None if !h.read_without_node(prop) => return Ok(()),
                None => {
                    // A reader of the field: the node, made with the value
                    // the field has now, re-runs it.
                    drop(store);
                    let n = self.ensure_node(obj, prop)?;
                    self.inner.graph.notify(n)?;
                    return Ok(());
                }
            }
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

    /// [`Engine::provenance`] as a tracked read: the binding or effect
    /// that asks runs again when the property is given its first value,
    /// also when that value is the one it had by default. For a default
    /// that means something else than the same value given on purpose
    /// (`Layout.leftMargin` follows `Layout.margins` until it is set).
    pub fn provenance_tracked(&self, obj: ObjectId, prop: PropIndex) -> Option<Provenance> {
        let (obj, prop) = self.redirect(obj, prop);
        if self.inner.tracking.get() == 0 {
            return self.provenance(obj, prop);
        }
        // A node is what a reader can depend on.
        let n = self.ensure_node(obj, prop).ok()?;
        self.inner.graph.provenance_tracked(n)
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
            let h = store
                .objects
                .get(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            self.check_writable(h, prop, true)?
        };
        let n = self.ensure_node(obj, prop)?;
        let g = &self.inner.graph;
        let value = match content {
            OverrideContent::Value(v) => {
                let v = self.for_slot(obj, prop, v);
                let v = coerce_for(&self.inner.store.borrow(), Some(obj), &ty, &v)?;
                OverrideValue::Value(v)
            }
            OverrideContent::Binding(mut f) => {
                let last = g.untrack(|| g.try_get(n)).unwrap_or_default();
                let f: BindingFn = Box::new(move |e| Some(f(e)));
                // An override's first value is put in place like a value
                // override is: no interceptor sees it.
                OverrideValue::binding(self.binding_wrapper(obj, prop, ty, last, f, Free::First))
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
                .ok_or_else(|| EngineError::dead(obj))?
                .node(prop)
        };
        let n = node.ok_or_else(|| {
            EngineError::Reactive(ljos_reactive::ReactiveError::UnknownOverride(
                NodeId::null(),
                id,
            ))
        })?;
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

    /// [`Engine::effect`] that dies with `obj`. Like `obj`'s bindings it
    /// stops once `obj`'s destruction has been told.
    pub fn effect_in(
        &self,
        obj: ObjectId,
        mut f: impl FnMut(&Engine) + 'static,
    ) -> Result<NodeId, EngineError> {
        let scope = self.ensure_scope(obj)?;
        self.effect_scoped(scope, move |e| {
            if !e.destruction_told(obj) {
                f(e);
            }
        })
    }

    /// Whether `obj` is gone or `Component.onDestruction` has been told for
    /// it: what it runs would see a half torn-down object.
    fn destruction_told(&self, obj: ObjectId) -> bool {
        self.inner
            .store
            .borrow()
            .objects
            .get(obj)
            .is_none_or(|h| h.flags.contains(ObjectFlags::DESTRUCTION_TOLD))
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
        let h = store
            .objects
            .get(obj)
            .ok_or_else(|| EngineError::dead(obj))?;
        h.meta
            .method_index(name)
            .ok_or_else(|| EngineError::UnknownMethod {
                ty: h.meta.name.to_string(),
                name: name.into(),
            })
    }

    /// Calls a native method. Arguments are coerced by the thunk. With a
    /// slow-call budget set, the watchdog times the thunk call and names
    /// one that ran over budget (slow.rs); a script method is not timed.
    pub fn invoke(
        &self,
        obj: ObjectId,
        method: MethodIndex,
        args: &[Value],
    ) -> Result<Value, EngineError> {
        let budget = self.inner.slow.budget();
        let (thunk, slow_names, data) = {
            let store = self.inner.store.borrow();
            let h = store
                .objects
                .get(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            let m = h
                .meta
                .method(method)
                .ok_or_else(|| EngineError::UnknownMethod {
                    ty: h.meta.name.to_string(),
                    name: format!("#{}", method.0),
                })?;
            // The names a slow call is recorded under; taken only while
            // the watchdog is on, so a disabled watchdog costs nothing here.
            let slow_names = (budget.is_some() && m.thunk.is_some())
                .then(|| (h.meta.name.clone(), m.name.clone()));
            let thunk = match m.thunk {
                Some(t) => Ok(t),
                None => Err(h
                    .ext
                    .as_ref()
                    .and_then(|e| e.methods.iter().find(|(i, _)| *i == method))
                    .map(|(_, f)| f.clone())
                    .ok_or_else(|| EngineError::NotNative(m.name.to_string()))?),
            };
            // Which parameters take data, for a method with any.
            let data = m
                .params
                .iter()
                .any(|(_, t)| matches!(t, TypeTag::Data))
                .then(|| {
                    m.params
                        .iter()
                        .map(|(_, t)| matches!(t, TypeTag::Data))
                        .collect::<Vec<_>>()
                });
            (thunk, slow_names, data)
        };
        let thunk = match thunk {
            Ok(t) => t,
            // A function declared in QML.
            Err(f) => return f(self, obj, args),
        };
        // An exception in what a native method calls is not its JS
        // caller's to catch, and a panic in it ends only this call
        // (fault.rs).
        self.inner.faults.propagate.set(false);
        // Native code reads lists, not a script's arrays, and data where
        // it takes data, not a script's objects.
        let args = match data {
            Some(data) => std::borrow::Cow::Owned(
                args.iter()
                    .enumerate()
                    .map(|(i, a)| match data.get(i) {
                        Some(true) => self.deep_copy(a),
                        _ => self.plain(a),
                    })
                    .collect(),
            ),
            None => self.plain_args(args),
        };
        // Only the thunk call below is timed, not the lookup or the
        // argument conversion above. This is `Instant`, not the engine
        // Clock: the virtual clock does not move during a call, and the
        // watchdog measures the machine, deliberately.
        let started = budget.map(|_| Instant::now());
        let result = self.guarded_result(obj, || thunk(&Cx::new(self, obj), &args));
        if let (Some(budget), Some(started), Some((ty, name))) = (budget, started, slow_names) {
            let took = started.elapsed();
            if took > budget {
                self.record_slow_call(ty, name, took, budget);
            }
        }
        result
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
            .ok_or_else(|| EngineError::dead(obj))?;
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

    /// Runs the native `on_complete` hooks, base type first, once: for an
    /// object made with [`Engine::create`], [`Engine::create_with`] or
    /// [`Engine::create_native_incomplete`]. Objects a component makes
    /// are completed when it is, and [`Engine::create_native`] and
    /// [`Engine::create_by_name`] complete theirs, so calling it on those
    /// does nothing.
    pub fn complete(&self, obj: ObjectId) -> Result<(), EngineError> {
        let hooks: SmallVec<[CompleteThunk; 4]> = {
            let mut store = self.inner.store.borrow_mut();
            let h = store
                .objects
                .get_mut(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            if h.flags.contains(ObjectFlags::COMPLETED) {
                return Ok(());
            }
            h.flags |= ObjectFlags::COMPLETED;
            let mut hooks: SmallVec<_> = h.meta.chain().filter_map(|m| m.on_complete).collect();
            hooks.reverse();
            hooks
        };
        // A view reads an object with hooks still to run as no rows
        // (`model_from_value`), unless it is a model already.
        let before = self.model_generation(obj);
        let pending = !hooks.is_empty() && before.is_some() && self.object_model(obj).is_none();
        let cx = Cx::new(self, obj);
        for f in hooks {
            self.guarded_result(obj, || f(&cx))?;
        }
        // A view that named this object before it was complete takes it
        // now, as a plain object unless its hooks made it a model (which
        // told the view already).
        if pending && self.model_generation(obj) == before {
            self.model_changed(obj)?;
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
        let (key, make) = attaching(attacher)
            .ok_or_else(|| EngineError::NoAttachedType(attacher.name.to_string()))?;
        {
            let store = self.inner.store.borrow();
            let h = store
                .objects
                .get(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            if let Some((_, a)) = h.attached.iter().find(|(k, _)| *k == key) {
                return Ok(*a);
            }
        }
        let a = self.create(make(), Some(obj))?;
        self.inner.store.borrow_mut().objects[obj]
            .attached
            .push((key, a));
        // Called unborrowed: a listener may attach something itself.
        let listeners = self.inner.attached_listeners.borrow().clone();
        let mut done = Vec::new();
        for l in &listeners {
            if !l(self, obj, a) {
                done.push(Rc::clone(l));
            }
        }
        if !done.is_empty() {
            self.inner
                .attached_listeners
                .borrow_mut()
                .retain(|l| !done.iter().any(|d| Rc::ptr_eq(d, l)));
        }
        Ok(a)
    }

    /// The object `attacher` has attached to `obj`, if one was made.
    /// Unlike [`Engine::attached_object`] this never makes one, so code
    /// that only looks (an accessibility pass over every item) does not
    /// give every object an attached object.
    pub fn existing_attached_object(&self, obj: ObjectId, attacher: &MetaRef) -> Option<ObjectId> {
        let (key, _) = attaching(attacher)?;
        let store = self.inner.store.borrow();
        let h = store.objects.get(obj)?;
        h.attached.iter().find(|(k, _)| *k == key).map(|(_, a)| *a)
    }

    /// Calls `f` after an attached object is first made, with the object
    /// and what was attached to it. Whoever looked with
    /// [`Engine::existing_attached_object`] and found nothing learns here
    /// that there is something now. `f` is forgotten once it returns
    /// false, so a listener whose owner is gone costs nothing afterwards.
    pub fn on_attached_created(&self, f: impl Fn(&Engine, ObjectId, ObjectId) -> bool + 'static) {
        self.inner.attached_listeners.borrow_mut().push(Rc::new(f));
    }

    // ------------------------------------------------------------ contexts

    pub fn root_context(&self) -> ContextId {
        self.inner.root_context
    }

    /// A child context (one per component instance).
    pub fn create_context(&self, parent: Option<ContextId>) -> ContextId {
        let serial = self.inner.context_serial.get() + 1;
        self.inner.context_serial.set(serial);
        self.inner.contexts.borrow_mut().insert(Context {
            parent: Some(parent.unwrap_or(self.inner.root_context)),
            serial,
            ..Context::default()
        })
    }

    /// A context's parent and age, for ordering.
    pub(crate) fn context_lineage(&self, ctx: ContextId) -> Option<(Option<ContextId>, u64)> {
        let contexts = self.inner.contexts.borrow();
        contexts.get(ctx).map(|c| (c.parent, c.serial))
    }

    pub fn destroy_context(&self, ctx: ContextId) {
        if ctx != self.inner.root_context {
            self.inner.contexts.borrow_mut().remove(ctx);
            self.release_scripts(ctx);
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

    /// A context property, looked up from `ctx` outwards: what was set by
    /// name, then the properties of the context's object (a delegate's
    /// row: `index`, `model`, the roles).
    pub fn context_property(&self, ctx: ContextId, name: &str) -> Option<Value> {
        let mut cur = Some(ctx);
        while let Some(id) = cur {
            // Not borrowed across the read below, which may run a binding.
            let object = {
                let contexts = self.inner.contexts.borrow();
                let c = contexts.get(id)?;
                if let Some(v) = c.props.get(name) {
                    return Some(v.clone());
                }
                cur = c.parent;
                c.context_object
            };
            if let Some(v) = object.and_then(|o| self.context_object_property(o, name)) {
                return Some(v);
            }
        }
        None
    }

    /// The property `name` of a context object; tracked.
    fn context_object_property(&self, obj: ObjectId, name: &str) -> Option<Value> {
        let p = self.property_index(obj, name).ok()?;
        self.try_read(obj, p).ok()
    }

    /// A name the compiler could not place, looked up from `ctx` outwards
    /// as Qt does at run time: the ids of each context, then its context
    /// properties. An inline component reads the ids of the file that
    /// uses it this way.
    pub(crate) fn context_name(&self, ctx: ContextId, name: &str) -> Option<Value> {
        let mut cur = Some(ctx);
        let found = loop {
            let object = {
                let contexts = self.inner.contexts.borrow();
                let c = contexts.get(cur?)?;
                if let Some(o) = c.ids.get(name) {
                    break *o;
                }
                if let Some(v) = c.props.get(name) {
                    return Some(v.clone());
                }
                cur = c.parent;
                c.context_object
            };
            if let Some(v) = object.and_then(|o| self.context_object_property(o, name)) {
                return Some(v);
            }
        };
        Some(if self.is_alive(found) {
            Value::Object(found)
        } else {
            Value::Null
        })
    }

    /// Where `name` is from `ctx` outwards: the context object that has a
    /// property of the name, or the value when it is a name a context was
    /// given outright (or, with `ids`, an id). As
    /// [`Engine::context_property`] and [`Engine::context_name`] look.
    pub(crate) fn context_holder(
        &self,
        ctx: ContextId,
        name: &str,
        ids: bool,
    ) -> Option<Result<ObjectId, Value>> {
        let mut cur = Some(ctx);
        while let Some(id) = cur {
            let object = {
                let contexts = self.inner.contexts.borrow();
                let c = contexts.get(id)?;
                if ids && let Some(o) = c.ids.get(name) {
                    return Some(Err(if self.is_alive(*o) {
                        Value::Object(*o)
                    } else {
                        Value::Null
                    }));
                }
                if let Some(v) = c.props.get(name) {
                    return Some(Err(v.clone()));
                }
                cur = c.parent;
                c.context_object
            };
            if let Some(o) = object
                && self.property_index(o, name).is_ok()
            {
                return Some(Ok(o));
            }
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
            .ok_or_else(|| EngineError::dead(obj))?
            .context = ctx;
        Ok(())
    }

    /// Has `component` make its objects in a child of `ctx` from now on.
    /// A view that makes a page from a file (a `StackView` given a URL)
    /// makes it in its own context, so the page sees the names around
    /// the view as it does in Qt; a file component alone sees only the
    /// root context.
    pub fn set_component_context(
        &self,
        component: ObjectId,
        ctx: ContextId,
    ) -> Result<(), EngineError> {
        let t = self.component_template(component)?;
        let t = crate::incubator::Template {
            file: t.file.clone(),
            comp: t.comp,
            outer: t.outer.clone(),
            context: Some(ctx),
        };
        self.with_mut::<crate::qtqml::Component, _>(component, |c| {
            c.template = Some(Rc::new(t));
        })
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

    /// Where `obj` was declared, as Qt writes it before an object's own
    /// warning (`file.qml:5:9: QML Image: Cannot open: …`): the file's URL,
    /// line and column, then `": "`. `None` for an object not made from QML.
    pub fn declared_at(&self, obj: ObjectId) -> Option<String> {
        self.location_of(obj)
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
/// `owner` has the property: its type has the enums a property of it may hold.
pub(crate) fn coerce_for(
    store: &Store,
    owner: Option<ObjectId>,
    ty: &TypeTag,
    v: &Value,
) -> Result<Value, EngineError> {
    if let TypeTag::Enum(name) = ty
        && let Some(h) = owner.and_then(|o| store.objects.get(o))
    {
        return Ok(h.meta.coerce_enum(name, v)?);
    }
    if let TypeTag::Flags(name) = ty
        && let Some(h) = owner.and_then(|o| store.objects.get(o))
    {
        return Ok(h.meta.coerce_flags(name, v)?);
    }
    let c = ty.coerce(v)?;
    if let (TypeTag::Object(Some(name)), Value::Object(id)) = (ty, &c) {
        match store.objects.get(*id) {
            None => return Ok(Value::Null),
            Some(h) if !h.meta.inherits_name(name) => {
                return Err(ConvertError::new(h.meta.name.to_string(), name.to_string()).into());
            }
            Some(_) => {}
        }
    }
    Ok(c)
}

pub(crate) fn read_access(
    native: &dyn QmlObject,
    dynamic: &[Value],
    access: &PropertyAccess,
) -> Value {
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

/// What a write of `v` to a native field stores: `v`, what the field's
/// [`WriteFilter`](crate::meta::WriteFilter) makes of it, or nothing if
/// the filter refuses it.
fn filter_access(
    native: &dyn QmlObject,
    access: &PropertyAccess,
    v: Value,
) -> Result<Option<Value>, ConvertError> {
    match *access {
        PropertyAccess::Field {
            owner,
            write: Some(write),
            ..
        } => match find_native(native, owner) {
            Some(o) => write(o, &v),
            None => Ok(Some(v)),
        },
        PropertyAccess::Group {
            owner,
            get,
            meta,
            leaf,
            ..
        } => match find_native(native, owner) {
            Some(o) => filter_access(get(o), &meta.properties[leaf.0 as usize].access, v),
            None => Ok(Some(v)),
        },
        _ => Ok(Some(v)),
    }
}

pub(crate) fn filter_in(
    h: &ObjectHeader,
    prop: PropIndex,
    v: Value,
) -> Result<Option<Value>, ConvertError> {
    match h.meta.property(prop) {
        Some(p) => filter_access(&*h.native, &p.access, v),
        None => Ok(Some(v)),
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

pub(crate) fn store_in(
    h: &mut ObjectHeader,
    prop: PropIndex,
    v: &Value,
) -> Result<(), ConvertError> {
    let Some(access) = h.meta.property(prop).map(|p| p.access) else {
        return Ok(());
    };
    write_access(&mut *h.native, &mut h.dynamic, &access, v)
}

/// The type among `attacher` and its bases that says what is attached, as
/// the key of what it attaches. A type declared in QML attaches what the
/// type it is made from does, and the same object: `ScrollBar.vertical`
/// names one object whether `ScrollBar` is the template or a style's
/// `ScrollBar.qml`.
fn attaching(attacher: &MetaRef) -> Option<(crate::meta::AttachedTypeId, fn() -> MetaRef)> {
    attacher.chain().find_map(|m| {
        let key = crate::meta::AttachedTypeId(std::ptr::from_ref(m) as usize);
        m.attached.map(|make| (key, make))
    })
}
