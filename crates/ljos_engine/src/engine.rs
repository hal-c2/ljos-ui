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
use std::rc::{Rc, Weak};
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
use crate::meta::{
    CompleteThunk, Declared, MetaObject, MetaRef, Prop, PropertyAccess, PropertyFlags, WriteFilter,
};
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

/// Called with the objects [`Engine::destroy_now`] took out of the store,
/// owners after what they own.
pub type DestroyListener = Rc<dyn Fn(&Engine, &[ObjectId])>;

/// Called with the root an incubation made, once it is whole.
pub type IncubatedListener = Rc<dyn Fn(&Engine, ObjectId)>;

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
    pub(crate) registry: RefCell<Registry>,
    /// Depth of engine-run bindings and effects on the stack; 0 inside
    /// handlers and `untrack`. A read creates a node only when this is
    /// non-zero, because only then is there a reader to depend on it.
    pub(crate) tracking: Cell<u32>,
    /// Depth of completion hooks on the stack.
    completing: Cell<u32>,
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
    /// The application font this engine set for its thread.
    pub(crate) default_font: crate::value::EngineFont,
    /// The value types modules registered (value_custom.rs).
    pub(crate) value_types: RefCell<Vec<&'static dyn crate::value_custom::CustomType>>,
    /// Read by every translated string; bumped by
    /// [`Engine::retranslate`].
    pub(crate) translation: Cell<Option<NodeId>>,
    pub(crate) parent_listeners: RefCell<Vec<ParentListener>>,
    pub(crate) destroy_listeners: RefCell<Vec<DestroyListener>>,
    pub(crate) incubated_listeners: RefCell<Vec<IncubatedListener>>,
    /// Each with the type of attached object it is for, if only one.
    pub(crate) attached_listeners: RefCell<Vec<(Option<&'static MetaObject>, AttachedListener)>>,
    /// Set by the first `Component.onDestruction` handler, so destroying
    /// objects costs nothing extra in an engine that has none.
    pub(crate) destruction_handlers: Cell<bool>,
    /// While a pass of deferred deletes runs, the survivors whose lists may
    /// hold the dead, cleaned once at the end instead of once for each
    /// object destroyed (`Engine::forget_in_lists`).
    pub(crate) list_holders: RefCell<Option<rustc_hash::FxHashSet<ObjectId>>>,
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
    /// The objects an inspector traces (trace.rs).
    pub(crate) traces: crate::trace::Traces,
    /// The model each view was given, for an inspector (model.rs).
    pub(crate) shown: crate::model::Shown,
    /// What a frame timeline counts (timing.rs); one flag while off.
    pub(crate) timing: crate::timing::Timing,
    /// Hot bindings and the binding loops seen (`profile.rs`).
    pub(crate) profile: crate::profile::Profiler,
    /// What each turn did, while an inspector asks (`journal.rs`).
    pub(crate) journal: crate::journal::JournalState,
    /// The properties whose setters (those that get a `Cx`) are running,
    /// innermost last: a write to one of them from inside is refused.
    setters: RefCell<SmallVec<[(ObjectId, PropIndex); 2]>>,
    /// How many objects [`Engine::set_frozen`] froze and did not thaw,
    /// dead ones too: none, and nothing is frozen (freeze.rs).
    pub(crate) frozen: Cell<u32>,
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
    /// A binding that can fail (a JS exception): `None` keeps the last
    /// value, as Qt does.
    FallibleBinding(BindingFn),
}

impl std::fmt::Debug for OverrideContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OverrideContent::Value(v) => f.debug_tuple("Value").field(v).finish(),
            OverrideContent::Binding(_) => f.write_str("Binding(..)"),
            OverrideContent::FallibleBinding(_) => f.write_str("FallibleBinding(..)"),
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
        // Most engines run scripts; the first one in a process has the
        // scripts every one runs compiled while it is being set up.
        crate::js::compile_globals_ahead();
        let graph = Graph::with_options(Options {
            equals: Value::write_equals,
            // Qt runs a handler that writes its own property again inside
            // itself until its script stack runs out, 835 runs deep on Qt
            // 6.11; a cycle that ends there ends here too.
            max_reruns_per_flush: 1000,
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
            completing: Cell::new(0),
            log: RefCell::new(LogState::default()),
            js: RefCell::new(None),
            loader: RefCell::new(crate::loader::LoaderState::default()),
            timers: RefCell::new(crate::qtqml::TimerState::default()),
            eager_bindings: Cell::new(false),
            eager_hold: RefCell::new(None),
            translation: Cell::new(None),
            default_font: Default::default(),
            value_types: RefCell::new(Vec::new()),
            parent_listeners: RefCell::new(Vec::new()),
            destroy_listeners: RefCell::new(Vec::new()),
            incubated_listeners: RefCell::new(Vec::new()),
            attached_listeners: RefCell::new(Vec::new()),
            destruction_handlers: Cell::new(false),
            list_holders: RefCell::new(None),
            context_serial: Cell::new(0),
            hooks: crate::hooks::Hooks::default(),
            reload: Default::default(),
            tier1: Default::default(),
            faults: Default::default(),
            slow: Default::default(),
            delayed: Default::default(),
            traces: Default::default(),
            shown: Default::default(),
            timing: Default::default(),
            profile: Default::default(),
            journal: Default::default(),
            setters: RefCell::new(SmallVec::new()),
            frozen: Cell::new(0),
        });
        let weak = Rc::downgrade(&inner);
        let noted = Rc::downgrade(&inner);
        inner.graph.set_note_handler(move |n| {
            if let Some(inner) = noted.upgrade() {
                Engine { inner }.noted(n);
            }
        });
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
        let slot = self.inner.registry.borrow().slot(uri, name)?;
        self.described(slot)
    }

    /// `name` in any registered module.
    pub fn lookup_type_any(&self, name: &str) -> Option<MetaRef> {
        let slot = self.inner.registry.borrow().slot_any(name)?;
        self.described(slot)
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
        let slot = self
            .inner
            .registry
            .borrow()
            .slot_versioned(uri, version, name)?;
        self.described(slot)
    }

    /// The `ljostypes.json` content for everything registered.
    pub fn type_dump(&self) -> TypeDump {
        let undescribed = self.inner.registry.borrow().undescribed();
        for key in undescribed {
            self.describe_qml_type(&key);
        }
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
        if let Diagnostic::BindingLoop { node, cycle } = d {
            self.loop_seen(*node, cycle);
        }
        let node = match d {
            Diagnostic::BindingLoop { node, .. } | Diagnostic::RerunLimit { node, .. } => *node,
        };
        let key = self.inner.store.borrow().node_keys.get(node).copied();
        // A handler is an effect with no property of its own: it is named
        // by the property whose change runs it.
        let watched = || {
            let store = self.inner.store.borrow();
            let sources = self.inner.graph.sources(node);
            sources
                .iter()
                .find_map(|s| store.node_keys.get(*s).copied())
        };
        let what = match (d, key) {
            (_, Some(k)) => Some(self.describe(k.object, k.prop)),
            (Diagnostic::RerunLimit { .. }, None) => {
                watched().map(|k| self.describe(k.object, k.prop))
            }
            (Diagnostic::BindingLoop { .. }, None) => None,
        };
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

    /// Warns of a binding loop through `prop` of `obj`, in Qt's words: for
    /// a loop a type finds itself, not one the graph does.
    pub fn warn_binding_loop(&self, obj: ObjectId, prop: PropIndex) {
        let msg = self.loop_message(obj, prop);
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
        let binding = h.binding_loc(prop);
        let place = |l: &Location| format!("{}:{}:{}", l.url, l.line, l.column);
        let object_at = h.location().as_ref().map(place);
        let binding_at = binding.as_ref().map(|b| place(&b.at));

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
        if let Some(member) = binding.as_ref().and_then(|b| b.member.as_ref()) {
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
    pub(crate) fn binding_place(&self, obj: ObjectId, prop: PropIndex) -> Option<String> {
        let store = self.inner.store.borrow();
        let l = store.objects.get(obj)?.binding_loc(prop)?.at;
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

    /// Live nodes of the reactive graph: a property something bound or
    /// read, a binding, an effect. Most of an engine's own memory goes
    /// with them, so a tool watching memory reads this.
    pub fn node_count(&self) -> usize {
        self.inner.graph.len()
    }

    pub fn meta_of(&self, obj: ObjectId) -> Option<MetaRef> {
        Some(self.inner.store.borrow().objects.get(obj)?.meta.clone())
    }

    /// [`MetaObject::class`] of `obj`'s type, `None` once it is gone. The
    /// description is looked at where it is: `make` runs with the store
    /// borrowed, so it asks the type and not the engine.
    pub fn class_of<T: Copy + Send + Sync + 'static>(
        &self,
        obj: ObjectId,
        make: impl FnOnce(&MetaObject) -> T,
    ) -> Option<T> {
        let store = self.inner.store.borrow();
        Some(store.objects.get(obj)?.meta.class(make))
    }

    /// Whether `obj` is alive and of the type `ty` or one derived from it:
    /// [`MetaObject::inherits`] of its [`meta_of`](Self::meta_of), with
    /// the description looked at where it is rather than shared out.
    pub fn inherits(&self, obj: ObjectId, ty: &MetaObject) -> bool {
        self.inner
            .store
            .borrow()
            .objects
            .get(obj)
            .is_some_and(|h| h.meta.inherits(ty))
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
            let first = store.deferred.is_empty();
            store.deferred.push(obj);
            drop(store);
            self.reload_doomed();
            // Nothing else may change: the host is woken for the turn
            // that deletes it.
            if first {
                self.wake_host();
            }
        }
        true
    }

    /// Whether objects wait for [`Engine::process_deferred_deletes`].
    pub fn has_deferred_deletes(&self) -> bool {
        !self.inner.store.borrow().deferred.is_empty()
    }

    /// Destroys everything scheduled by [`Engine::destroy`]. Returns how
    /// many scheduled objects were destroyed.
    pub fn process_deferred_deletes(&self) -> usize {
        let mut n = 0;
        // A pass inside a pass (a handler that processes them) shares it.
        let outer = self.inner.list_holders.borrow().is_none();
        if outer {
            *self.inner.list_holders.borrow_mut() = Some(Default::default());
        }
        loop {
            let batch = std::mem::take(&mut self.inner.store.borrow_mut().deferred);
            if batch.is_empty() {
                break;
            }
            for obj in batch {
                n += usize::from(self.destroy_now(obj));
            }
        }
        if outer {
            self.forget_held_lists();
            *self.inner.list_holders.borrow_mut() = None;
        }
        n
    }

    /// Cleans the lists of the holders a pass of deferred deletes put off,
    /// before anything that could read them runs.
    pub(crate) fn forget_held_lists(&self) {
        let holders = match self.inner.list_holders.borrow_mut().as_mut() {
            Some(h) if !h.is_empty() => std::mem::take(h),
            _ => return,
        };
        self.inner.graph.batch(|| {
            for p in holders {
                self.forget_in_lists(p);
            }
        });
    }

    /// Runs the `destroyed` handlers of `obj` and what it owns, owners
    /// first.
    fn tell_destroyed(&self, obj: ObjectId) {
        let handlers = {
            let store = self.inner.store.borrow();
            let mut order: SmallVec<[ObjectId; 8]> = SmallVec::new();
            let mut stack: SmallVec<[ObjectId; 8]> = SmallVec::new();
            stack.push(obj);
            while let Some(id) = stack.pop() {
                if let Some(h) = store.objects.get(id) {
                    order.push(id);
                    stack.extend(h.children.iter().rev().copied());
                }
            }
            drop(store);
            self.inner.signals.borrow_mut().take_destroyed(&order)
        };
        if !handlers.is_empty() {
            self.forget_held_lists();
        }
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
        self.tell_destruction_going(obj);
        self.tell_destroyed(obj);
        // Post-order: children before their owner. A view's delegate is a
        // handful of objects, which these hold without an allocation.
        let order = {
            let store = self.inner.store.borrow();
            if !store.objects.contains_key(obj) {
                return false;
            }
            let mut order: SmallVec<[ObjectId; 8]> = SmallVec::new();
            let mut stack: SmallVec<[(ObjectId, bool); 8]> = SmallVec::new();
            stack.push((obj, false));
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
        // A window's singletons are told it goes while it stands, and go
        // once it has.
        let scoped = self.singleton_scopes_end(&order);
        let mut headers = Vec::with_capacity(order.len());
        // Survivors whose visual parent died, to tell listeners.
        let mut orphans = Vec::new();
        // Survivors that may still list the dead: the owner and the
        // visual parent of the object destroyed, and the visual parent of
        // anything it owns that was put in another item (a popup's item in
        // the overlay).
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
                headers.push((*id, h));
            }
            if let Some((_, h)) = headers.last()
                && let Some(p) = h.owner
                && s.objects.contains_key(p)
            {
                holders.push(p);
            }
            // Each one shown in an item other than its owner (a popup in the
            // overlay) leaves that item's lists too.
            for p in headers.iter().filter_map(|(_, h)| h.visual_parent) {
                if s.objects.contains_key(p) && !holders.contains(&p) {
                    holders.push(p);
                }
            }
            // Unlink survivors from the dead. A survivor lists only the dead
            // it holds directly, each a header here, so each is taken out by
            // itself: a holder of many siblings destroyed one by one (a
            // board cleared for a new game) is not scanned whole for each.
            for (id, h) in &headers {
                if let Some(o) = h.owner.and_then(|o| s.objects.get_mut(o)) {
                    take_out(&mut o.children, *id);
                }
                if let Some(p) = h.visual_parent.and_then(|p| s.objects.get_mut(p)) {
                    take_out(&mut p.visual_children, *id);
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
        self.inner.shown.forget(&order);
        if self.inner.reload.tracking.get() {
            self.reload_forget(&order);
        }
        let js = self.inner.js.borrow().clone();
        if let Some(js) = js {
            js.forget(&order);
        }
        self.inner.graph.batch(|| {
            for (_, h) in &headers {
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
            // In a pass of deferred deletes a holder of many of the dead is
            // cleaned once, not once for each.
            let mut held = self.inner.list_holders.borrow_mut();
            match held.as_mut() {
                Some(pending) => {
                    pending.extend(holders);
                }
                None => {
                    drop(held);
                    for p in &holders {
                        self.forget_in_lists(*p);
                    }
                }
            }
        });
        let listeners = self.inner.destroy_listeners.borrow().clone();
        for l in listeners {
            l(self, &order);
        }
        for (_, h) in &headers {
            if let Some(ext) = &h.ext {
                for c in &ext.contexts {
                    self.destroy_context(*c);
                }
            }
        }
        // Natives and handlers may own anything; drop them unborrowed.
        drop(connections);
        drop(headers);
        for obj in scoped {
            self.destroy_now(obj);
        }
        true
    }

    /// Takes the dead out of the native list properties of `holder`
    /// (`Item.children`, `resources`, `data`), as Qt's own lists drop a
    /// deleted object. A `property list<T>` written in QML keeps its
    /// length and reads `null` there, so it is left alone.
    fn forget_in_lists(&self, holder: ObjectId) {
        // An item has more than four (`children`, `resources`, `data`,
        // `states`, ...), which spilled out of a smaller array each time.
        let lists: SmallVec<[PropIndex; 8]> = {
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
            // The dead are out of the store by now: asking it is one look
            // per entry, where the destroyed subtree is many.
            let mut dead: SmallVec<[ObjectId; 4]> = {
                let store = self.inner.store.borrow();
                l.iter()
                    .filter_map(Value::as_object)
                    .filter(|o| !store.objects.contains_key(*o))
                    .collect()
            };
            if dead.is_empty() {
                continue;
            }
            // A pass of deferred deletes may leave hundreds dead in one list
            // (a board cleared): each entry is looked up, not compared with
            // every one of them.
            dead.sort_unstable();
            // The read's share goes first, so the edit is made in place.
            drop(l);
            let _ = self.list_edit(holder, p, &|items| {
                items.retain(|v| {
                    v.as_object()
                        .is_none_or(|o| dead.binary_search(&o).is_err())
                });
            });
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
        self.set_visual_parent_at(obj, parent, None)
    }

    /// [`Engine::set_visual_parent`], `obj` going right before `anchor`
    /// among `parent`'s [visual children](Engine::visual_children), as it
    /// does in the item's own lists; at their end when `anchor` is not
    /// among them. A child of `parent` already only moves among its
    /// siblings, and nobody hears of it.
    pub fn set_visual_parent_before(
        &self,
        obj: ObjectId,
        parent: ObjectId,
        anchor: ObjectId,
    ) -> Result<(), EngineError> {
        self.set_visual_parent_at(obj, Some(parent), Some(anchor))
    }

    fn set_visual_parent_at(
        &self,
        obj: ObjectId,
        parent: Option<ObjectId>,
        anchor: Option<ObjectId>,
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
            if old == parent && anchor.is_none() {
                return Ok(());
            }
            if let Some(h) = old.and_then(|o| store.objects.get_mut(o)) {
                h.visual_children.retain(|c| *c != obj);
            }
            if let Some(p) = parent {
                let kids = &mut store.objects[p].visual_children;
                match anchor.and_then(|a| kids.iter().rposition(|c| *c == a)) {
                    Some(at) => kids.insert(at, obj),
                    None => kids.push(obj),
                }
            }
            // Only its place among its siblings changed.
            if old == parent {
                return Ok(());
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

    /// Calls `f` each time [`Engine::destroy_now`] has taken objects out
    /// of the store, with all of them: a scene drops what it showed of
    /// them without a reactive node for each that it shows.
    pub fn on_destroyed(&self, f: impl Fn(&Engine, &[ObjectId]) + 'static) {
        self.inner.destroy_listeners.borrow_mut().push(Rc::new(f));
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

    /// Whether `obj` is alive and has a `T` part. Unlike a [`with`](Self::with)
    /// that fails, it builds no error: a question asked of every delegate.
    pub fn is<T: QmlType>(&self, obj: ObjectId) -> bool {
        self.inner
            .store
            .borrow()
            .objects
            .get(obj)
            .is_some_and(|h| downcast_chain::<T>(&*h.native).is_some())
    }

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
        let t = downcast_chain::<T>(&*h.native)
            .ok_or_else(|| wrong_type(&h.meta.name, T::static_meta()))?;
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
        let t = downcast_chain_mut::<T>(&mut *h.native)
            .ok_or_else(|| wrong_type(&name, T::static_meta()))?;
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

    /// `read(obj, prop).to_number()`, without the value made and dropped
    /// on the way when the property holds a number: native code reads the
    /// geometry of every item that moved, every frame.
    pub fn read_number(&self, obj: ObjectId, prop: PropIndex) -> f64 {
        self.read_numbers(obj, [prop]).map_or(0.0, |[v]| v)
    }

    /// [`Engine::read_number`] of each of `props` of `obj`, with the
    /// object looked up once; `None` when it is gone. A scene reads where
    /// each item that moved is, every frame. What only its field holds is
    /// read as the object is looked up, before a binding of another of
    /// `props` that is out of date runs.
    pub fn read_numbers<const N: usize>(
        &self,
        obj: ObjectId,
        props: [PropIndex; N],
    ) -> Option<[f64; N]> {
        let fast = |v: &Value| match v {
            Value::Real(r) => Some(*r),
            Value::Int(i) => Some(f64::from(*i)),
            _ => None,
        };
        // A plain loop: `props.map` was a call per property and more.
        let mut nodes = [None; N];
        let mut out = [None; N];
        let mut kept = [false; N];
        {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj)?;
            let tracking = self.inner.tracking.get() != 0;
            for i in 0..N {
                let prop = props[i];
                let Some(p) = h.meta.property(prop) else {
                    continue;
                };
                if p.is_alias() || h.makes_on_read(prop) {
                    continue;
                }
                match h.node(prop) {
                    Some(n) => nodes[i] = Some(n),
                    None => {
                        out[i] = fast(&read_access(&*h.native, &h.dynamic, &p.access));
                        kept[i] = out[i].is_some()
                            && tracking
                            && !p
                                .flags
                                .intersects(PropertyFlags::CONSTANT | PropertyFlags::GROUP);
                    }
                }
            }
        }
        if kept.contains(&true) {
            self.keep_field_reads(obj, (0..N).filter(|&i| kept[i]).map(|i| props[i]));
        }
        let mut numbers = [0.0; N];
        for i in 0..N {
            let v = match nodes[i] {
                Some(n) => self.inner.graph.try_get_with(n, fast).flatten(),
                None => out[i],
            };
            numbers[i] = v.unwrap_or_else(|| self.read(obj, props[i]).to_number());
        }
        Some(numbers)
    }

    /// [`Engine::read`] of each of `props` of `obj`, with the object
    /// looked up once and the tracked reads of fields kept together: a
    /// scene reads the forty inputs of a text, or a rectangle's paint, as
    /// each is made. What only its field holds is read as the object is
    /// looked up, before a binding of another of `props` that is out of
    /// date runs.
    pub fn read_values<const N: usize>(&self, obj: ObjectId, props: [PropIndex; N]) -> [Value; N] {
        self.read_values_impl::<N, false>(obj, props).0
    }

    /// [`Engine::read_values`], with `None` for each of `props` still to be
    /// bound on its read ([`Engine::bind_on_read`]), which stays unbound:
    /// [`Engine::read_unless_bound_on_read`] of each, together.
    pub fn read_values_unless_bound_on_read<const N: usize>(
        &self,
        obj: ObjectId,
        props: [PropIndex; N],
    ) -> [Option<Value>; N] {
        let (mut out, due) = self.read_values_impl::<N, true>(obj, props);
        std::array::from_fn(|i| (!due[i]).then(|| std::mem::take(&mut out[i])))
    }

    fn read_values_impl<const N: usize, const UNBOUND: bool>(
        &self,
        obj: ObjectId,
        props: [PropIndex; N],
    ) -> ([Value; N], [bool; N]) {
        #[derive(Clone, Copy)]
        enum Slot {
            Node(NodeId),
            Read,
            /// Still to be bound on its read; left so.
            Due,
            /// As [`Engine::read`] does it.
            Slow,
        }
        let mut out: [Value; N] = std::array::from_fn(|_| Value::Undefined);
        let mut slots = [Slot::Slow; N];
        let mut kept = [false; N];
        {
            let store = self.inner.store.borrow();
            let Some(h) = store.objects.get(obj) else {
                return (std::array::from_fn(|_| Value::Null), [false; N]);
            };
            let tracking = self.inner.tracking.get() != 0;
            for i in 0..N {
                let prop = props[i];
                let Some(p) = h.meta.property(prop) else {
                    continue;
                };
                if UNBOUND && !p.is_alias() && h.binds_on_read(prop) {
                    // A write drops the binding to come, and the reader
                    // hears of it as of any field it read.
                    kept[i] = tracking;
                    slots[i] = Slot::Due;
                    continue;
                }
                if p.is_alias() || h.makes_on_read(prop) {
                    continue;
                }
                match h.node(prop) {
                    Some(n) => slots[i] = Slot::Node(n),
                    None => {
                        let v = read_access(&*h.native, &h.dynamic, &p.access);
                        // A record is made a node on its first read.
                        if matches!(v, Value::Map(_)) {
                            continue;
                        }
                        kept[i] = tracking
                            && !p
                                .flags
                                .intersects(PropertyFlags::CONSTANT | PropertyFlags::GROUP);
                        out[i] = v;
                        slots[i] = Slot::Read;
                    }
                }
            }
        }
        if kept.contains(&true) {
            self.keep_field_reads(obj, (0..N).filter(|&i| kept[i]).map(|i| props[i]));
        }
        let mut due = [false; N];
        for i in 0..N {
            let v = match slots[i] {
                Slot::Node(n) => self.inner.graph.try_get(n).unwrap_or_default(),
                Slot::Read => std::mem::take(&mut out[i]),
                Slot::Due => {
                    due[i] = true;
                    continue;
                }
                Slot::Slow => {
                    out[i] = self.read(obj, props[i]);
                    continue;
                }
            };
            out[i] = self.live(v);
        }
        (out, due)
    }

    /// [`Engine::property_index`] for a caller that has no use for why
    /// there is none: no error is built.
    pub fn find_property(&self, obj: ObjectId, name: &str) -> Option<PropIndex> {
        self.inner
            .store
            .borrow()
            .objects
            .get(obj)?
            .meta
            .property_index(name)
    }

    pub fn read_by_name(&self, obj: ObjectId, name: &str) -> Value {
        let found = {
            let store = self.inner.store.borrow();
            store.objects.get(obj).map(|h| h.meta.property_index(name))
        };
        match found {
            Some(Some(p)) => self.read(obj, p),
            None => Value::Null,
            Some(None) => Value::Undefined,
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
            /// To be made first.
            Due,
        }
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
            if h.makes_on_read(prop) {
                Got::Due
            } else if p.is_alias() {
                Got::Alias(h.alias(prop))
            } else {
                match h.node(prop) {
                    Some(n) => Got::Node(n),
                    // One only its making writes holds, with no node, the
                    // value it was made with, before anything reads it: a
                    // read of it needs no keeping.
                    None if self.inner.tracking.get() == 0
                        || p.flags.intersects(
                            PropertyFlags::CONSTANT
                                | PropertyFlags::GROUP
                                | PropertyFlags::MADE_ONLY,
                        ) =>
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
            // The maker goes as it runs, so this reads on once.
            Got::Due => {
                self.make_if_due(obj, prop);
                return self.try_read(obj, prop);
            }
        };
        Ok(self.live(v))
    }

    /// What [`try_read`](Self::try_read) gives a script's read of a plain
    /// property with a node, in one look at the object, when it is a
    /// number, a bool or a text; `None` sends the read the long way, which
    /// sees to aliases, groups, a `list<T>`, makers, lists and objects.
    pub(crate) fn plain_script_read(&self, obj: ObjectId, prop: PropIndex) -> Option<Value> {
        self.quick_script_read(obj, prop, false)
    }

    /// As [`plain_script_read`](Self::plain_script_read), and with
    /// `objects` a reference to an object too, as `try_read` gives it.
    pub(crate) fn quick_script_read(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        objects: bool,
    ) -> Option<Value> {
        let quick = |v: Value| match v {
            Value::Object(_) if objects => Some(self.live(v)),
            v => primitive(&v).then_some(v),
        };
        let n = {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj)?;
            let p = h.meta.property(prop)?;
            if p.is_alias()
                || p.flags.contains(PropertyFlags::GROUP)
                || matches!(p.ty, TypeTag::List(_))
                || h.makes_on_read(prop)
            {
                return None;
            }
            match h.node(prop) {
                Some(n) => n,
                // As try_read reads a field nobody follows.
                None if self.inner.tracking.get() == 0
                    || p.flags.contains(PropertyFlags::CONSTANT) =>
                {
                    let v = read_access(&*h.native, &h.dynamic, &p.access);
                    drop(store);
                    return quick(v);
                }
                // Kept on the object, as try_read keeps it.
                None => {
                    let v = read_access(&*h.native, &h.dynamic, &p.access);
                    drop(store);
                    let v = quick(v)?;
                    self.keep_field_read(obj, prop);
                    return Some(v);
                }
            }
        };
        quick(self.inner.graph.try_get(n)?)
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

    /// Whether the property, or what it is an alias of, is a list.
    pub(crate) fn is_list_property(&self, obj: ObjectId, prop: PropIndex) -> bool {
        let (obj, prop) = self.redirect(obj, prop);
        let store = self.inner.store.borrow();
        store
            .objects
            .get(obj)
            .and_then(|h| h.meta.property(prop))
            .is_some_and(|p| matches!(p.ty, TypeTag::List(_)))
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
    /// [`Engine::provenance`] says `Default` or `Host` afterwards,
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
                match filter_in(h, prop, v)? {
                    Filtered::Done(Some(v)) => {
                        store_in(h, prop, &v)?;
                        let change = change_hook(h, prop);
                        drop(store);
                        self.tell_change(obj, prop, change);
                    }
                    Filtered::Done(None) => {}
                    // The setter may write other properties, so it runs
                    // where a write can: unfiltered, through the full path.
                    Filtered::WithCx(_, v) => {
                        drop(store);
                        return self.write_impl(obj, prop, v, WriteKind::Host, true);
                    }
                }
                return Ok(());
            }
        }
        self.write_impl(obj, prop, v, WriteKind::Host, true)
    }

    fn check_writable<'h>(
        &self,
        h: &'h ObjectHeader,
        prop: PropIndex,
        internal: bool,
    ) -> Result<&'h TypeTag, EngineError> {
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
        Ok(&p.ty)
    }

    pub(crate) fn write_impl(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        v: Value,
        how: WriteKind,
        internal: bool,
    ) -> Result<(), EngineError> {
        // An animation step, every frame for each animated property, or
        // the toolkit keeping a number of its own (a model's count): a
        // number to a number property with no setter of its own. One look
        // says so, and nothing between would change it.
        if internal
            && matches!(how, WriteKind::Animation | WriteKind::Imperative)
            && let Some(at) = self.plain_number(obj, prop, &v, how)
        {
            return match at {
                PlainNumber::Node(n) => self.write_node(obj, prop, n, v, how),
                PlainNumber::Field => self.store_field(obj, prop, &v),
            };
        }
        // Nearly every write is to a property that is no alias and has
        // nothing still to be made: one look says so.
        let (obj, prop) = match self.write_look(obj, prop) {
            WriteLook::Alias => {
                if let Some(target) = self.alias_member(obj, prop) {
                    return self.write_member(target, &v, how, internal);
                }
                let (obj, prop) = self.redirect(obj, prop);
                self.drop_maker(obj, prop);
                (obj, prop)
            }
            WriteLook::Maker => {
                self.drop_maker(obj, prop);
                (obj, prop)
            }
            WriteLook::Plain => (obj, prop),
        };
        // `x = undefined` takes a property with a reset back, and so does
        // a value that stands for none.
        if matches!(how, WriteKind::Imperative | WriteKind::Initial)
            && self.takes_back(obj, prop, &v)
        {
            return self.reset_impl(obj, prop, internal);
        }
        // An object of a group's type given to a group that is given whole:
        // `palette: Palette {}` follows it, as a group given in QML does.
        if let Value::Object(src) = v
            && self.is_whole_group(obj, prop)
        {
            return self.assign_group(obj, prop, src, "", how == WriteKind::Initial);
        }
        // An array is the script's own in a `var`, a list anywhere else
        // (script_arrays.rs).
        let v = self.for_slot(obj, prop, v);
        let (filtered, node, hook, given) = {
            let store = self.inner.store.borrow();
            let h = store
                .objects
                .get(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            let ty = self.check_writable(h, prop, internal)?;
            let (node, hook) = node_and_hook(h, prop, internal);
            // What an author gives: an animation, a layout or the toolkit
            // writing is not the property being given.
            let given = h.meta.property(prop).and_then(|p| p.given).filter(|_| {
                matches!(
                    how,
                    WriteKind::Imperative | WriteKind::Initial | WriteKind::Override(_)
                )
            });
            let v = coerce_for(&store, Some(obj), ty, &v)?;
            // The type's own say comes before anything hears of it, as a
            // setter's does in Qt.
            (filter_in(h, prop, v)?, node, hook, given)
        };
        let (v, node, hook) = match filtered {
            Filtered::Done(None) => return Ok(()),
            Filtered::Done(Some(v)) => (v, node, hook),
            setter @ Filtered::WithCx(..) => {
                let Some(v) = self.finish_filter(obj, prop, setter)? else {
                    return Ok(());
                };
                // What the setter wrote may have given the property a
                // node or a reader.
                let store = self.inner.store.borrow();
                let h = store
                    .objects
                    .get(obj)
                    .ok_or_else(|| EngineError::dead(obj))?;
                let (node, hook) = node_and_hook(h, prop, internal);
                (v, node, hook)
            }
        };
        let r = self.write_filtered(obj, prop, v, (node, hook), how, internal);
        if r.is_ok()
            && let Some(given) = given
        {
            self.untrack(|| given(&Cx::new(self, obj), prop));
        }
        r
    }

    /// The rest of [`write_impl`](Self::write_impl), once the type has had
    /// its say: the value goes to a model row, a Behavior, the node or the
    /// field.
    fn write_filtered(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        v: Value,
        (node, hook): (Result<Option<NodeId>, ()>, Option<crate::object::WriteHook>),
        how: WriteKind,
        internal: bool,
    ) -> Result<(), EngineError> {
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
            // An override entry only exists on a node, and so do the
            // host's and an animation's marks: without one the value would
            // read as assigned, and what follows a property until its
            // author sets it (a control's font, a style's opacity) would
            // stop following.
            (None, WriteKind::Override(_) | WriteKind::Host | WriteKind::Animation) => {
                self.ensure_node(obj, prop)?
            }
            // Journaled, a change is the graph's to keep: it gets a node.
            (None, _) if self.is_journaled() => self.ensure_node(obj, prop)?,
            (None, _) => return self.store_field(obj, prop, &v),
        };
        self.write_node(obj, prop, n, v, how)
    }

    /// A write to a property nobody observes: the field is the only copy.
    fn store_field(&self, obj: ObjectId, prop: PropIndex, v: &Value) -> Result<(), EngineError> {
        let mut store = self.inner.store.borrow_mut();
        let h = store
            .objects
            .get_mut(obj)
            .ok_or_else(|| EngineError::dead(obj))?;
        store_in(h, prop, v)?;
        h.mark_assigned(prop);
        let change = change_hook(h, prop);
        drop(store);
        self.tell_change(obj, prop, change);
        Ok(())
    }

    /// Where a write of `v` to `prop` goes when it needs no conversion,
    /// alias, setter, author's hook or value still to be made: the node
    /// as it is, or for a write that is no animation step, the field of
    /// a property no node or reader follows while nothing is journaled.
    fn plain_number(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        v: &Value,
        how: WriteKind,
    ) -> Option<PlainNumber> {
        let store = self.inner.store.borrow();
        let h = store.objects.get(obj)?;
        let p = h.meta.property(prop)?;
        let same = matches!(
            (&p.ty, v),
            (TypeTag::Real, Value::Real(_)) | (TypeTag::Int, Value::Int(_))
        );
        let unfiltered = matches!(
            p.access,
            PropertyAccess::Field { write: None, .. } | PropertyAccess::Dynamic { .. }
        );
        let animation = matches!(how, WriteKind::Animation);
        if !same
            || !unfiltered
            || p.is_alias()
            || p.flags.contains(PropertyFlags::GROUP)
            || (!animation && p.given.is_some())
        {
            return None;
        }
        if h.makes_on_read(prop) {
            return None;
        }
        match h.node(prop) {
            Some(n) => Some(PlainNumber::Node(n)),
            None if animation || h.read_without_node(prop) || self.is_journaled() => None,
            None => Some(PlainNumber::Field),
        }
    }

    /// A script's write of `v` that needs nothing between: a number, a
    /// bool or a text to a property of that kind with no alias, setter,
    /// model row, Behavior, reset or value still to be made. It is made
    /// here, and true comes back; what [`write_impl`](Self::write_impl)
    /// does with such a write comes to the same, after a look for each.
    pub(crate) fn plain_script_write(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        v: &Value,
    ) -> Result<bool, EngineError> {
        let (node, v) = {
            let store = self.inner.store.borrow();
            let Some(h) = store.objects.get(obj) else {
                return Ok(false);
            };
            let Some(p) = h.meta.property(prop) else {
                return Ok(false);
            };
            let kind = matches!(
                (&p.ty, v),
                (TypeTag::Real | TypeTag::Int, Value::Real(_) | Value::Int(_))
                    | (TypeTag::Bool, Value::Bool(_))
                    | (TypeTag::String, Value::String(_))
            );
            let unfiltered = matches!(
                p.access,
                PropertyAccess::Field { write: None, .. } | PropertyAccess::Dynamic { .. }
            );
            if !kind
                || !unfiltered
                || p.is_alias()
                || p.is_readonly()
                || p.flags
                    .intersects(PropertyFlags::GROUP | PropertyFlags::CONSTANT)
                || p.given.is_some()
                || h.makes_on_read(prop)
                || h.ext.as_ref().is_some_and(|x| x.write_hook.is_some())
                || h.flags.contains(ObjectFlags::INTERCEPTED)
                || matches!(v, Value::String(s) if s.is_empty())
            {
                return Ok(false);
            }
            let node = h.node(prop);
            // A reader follows it without a node, or the change is to be
            // journaled: the long way makes one.
            if node.is_none() && (h.read_without_node(prop) || self.is_journaled()) {
                return Ok(false);
            }
            let Ok(v) = p.ty.coerce(v) else {
                return Ok(false);
            };
            (node, v)
        };
        match node {
            // The value it was given again: a handler that sets what it
            // set last time changes nothing, and nothing need be flushed.
            Some(n) if self.inner.graph.holds_assigned(n, &v) => {}
            Some(n) => self.write_node(obj, prop, n, v, WriteKind::Imperative)?,
            None => {
                // Nobody observes it: the field is the only copy.
                let mut store = self.inner.store.borrow_mut();
                let h = store
                    .objects
                    .get_mut(obj)
                    .ok_or_else(|| EngineError::dead(obj))?;
                store_in(h, prop, &v)?;
                h.mark_assigned(prop);
                let change = change_hook(h, prop);
                drop(store);
                self.tell_change(obj, prop, change);
            }
        }
        Ok(true)
    }

    fn write_node(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        n: NodeId,
        v: Value,
        how: WriteKind,
    ) -> Result<(), EngineError> {
        let g = &self.inner.graph;
        let write = || {
            g.write(n, v, how)?;
            if let Some(cur) = g.settled(n) {
                self.store_value(obj, prop, &cur);
            }
            Ok(())
        };
        // Batched so effects run after the field cache is updated; in an
        // open batch they wait already.
        if g.batching() {
            write()
        } else {
            g.batch(write)
        }
    }

    /// Runs a setter that gets a [`Cx`], with no store borrow held, so it
    /// can read and write the object's other properties. Its reads are not
    /// tracked: a binding of the property does not come to depend on
    /// them. A write to the property itself from inside is refused.
    fn finish_filter(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        filtered: Filtered,
    ) -> Result<Option<Value>, EngineError> {
        let (setter, v) = match filtered {
            Filtered::Done(v) => return Ok(v),
            Filtered::WithCx(setter, v) => (setter, v),
        };
        if self.inner.setters.borrow().contains(&(obj, prop)) {
            let (ty, name) = self.describe(obj, prop);
            return Err(EngineError::WriteInOwnSetter {
                ty: ty.to_string(),
                name: name.to_string(),
            });
        }
        struct Running<'a>(&'a RefCell<SmallVec<[(ObjectId, PropIndex); 2]>>);
        impl Drop for Running<'_> {
            fn drop(&mut self) {
                self.0.borrow_mut().pop();
            }
        }
        self.inner.setters.borrow_mut().push((obj, prop));
        let _running = Running(&self.inner.setters);
        Ok(self.untrack(|| setter(&Cx::new(self, obj), &v))?)
    }

    /// What a write to `prop` of `obj` has to see to before the value is
    /// its own: a dead object is plain, and the write finds it dead.
    fn write_look(&self, obj: ObjectId, prop: PropIndex) -> WriteLook {
        let store = self.inner.store.borrow();
        let Some(h) = store.objects.get(obj) else {
            return WriteLook::Plain;
        };
        if h.meta.property(prop).is_some_and(|p| p.is_alias()) {
            WriteLook::Alias
        } else if h.makes_on_read(prop) {
            WriteLook::Maker
        } else {
            WriteLook::Plain
        }
    }

    /// Tells `change` (the property's hook) that the property changed.
    /// A node's changes the graph tells ([`Engine::noted`]); a field
    /// without one, whoever wrote it.
    pub(crate) fn tell_change(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        change: Option<crate::ChangeHook>,
    ) {
        if let Some(f) = change {
            self.untrack(|| f(&Cx::new(self, obj), prop));
        }
    }

    /// The graph told of a change to the noted node `n`: the property it
    /// is has a change hook, which is told.
    fn noted(&self, n: NodeId) {
        let change = {
            let store = self.inner.store.borrow();
            let Some(k) = store.node_keys.get(n) else {
                return;
            };
            let Some(h) = store.objects.get(k.object) else {
                return;
            };
            change_hook(h, k.prop).map(|f| (f, k.object, k.prop))
        };
        if let Some((f, obj, prop)) = change {
            self.tell_change(obj, prop, Some(f));
        }
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
        let (scope, value, (assigned, noted)) = {
            let store = self.inner.store.borrow();
            let h = store
                .objects
                .get(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            if let Some(n) = h.node(prop) {
                return Ok(n);
            }
            if h.binds_on_read(prop) {
                drop(store);
                // Observed from now on: the binding it was to get on its
                // first read is made, with the node.
                self.make_if_due(obj, prop);
                return self.ensure_node(obj, prop);
            }
            let p = h
                .meta
                .property(prop)
                .ok_or_else(|| unknown_index(&h.meta, prop))?;
            (
                h.scope,
                read_access(&*h.native, &h.dynamic, &p.access),
                (h.is_assigned(prop), p.change.is_some()),
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
        if noted {
            g.set_noted(n)?;
        }
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
    pub(crate) fn keep_field_read(&self, obj: ObjectId, prop: PropIndex) {
        self.keep_field_reads(obj, std::iter::once(prop));
    }

    /// [`Engine::keep_field_read`] of each of `props`, with the reader and
    /// the object looked up once.
    fn keep_field_reads(&self, obj: ObjectId, props: impl Iterator<Item = PropIndex>) {
        let g = &self.inner.graph;
        let Some(r) = g.reader() else {
            return;
        };
        let mut store = self.inner.store.borrow_mut();
        if let Some(h) = store.objects.get_mut(obj) {
            let len = h.meta.properties.len();
            let reads = h
                .field_reads
                .get_or_insert_with(|| Box::new(FieldReads::new(len)));
            for prop in props {
                reads.add(prop, r, |e| g.is_current(e));
            }
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
        self.bind_impl(obj, prop, move |e| Some(f(e)), false)
    }

    /// [`Engine::bind`] for what a native type gives a property it was not
    /// given: does nothing when the property has a value of its own. A
    /// reload that takes that value away binds it to `f` again, as a new
    /// object would be.
    pub fn bind_default(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        f: impl Fn(&Engine) -> Value + 'static,
    ) -> Result<(), EngineError> {
        let f: crate::incubator::reload::DefaultBinding = Rc::new(f);
        self.reload_default(obj, prop, &f);
        if matches!(
            self.provenance(obj, prop),
            Some(Provenance::Default | Provenance::Animated | Provenance::Host) | None
        ) {
            self.bind_impl(obj, prop, move |e| Some(f(e)), false)?;
        }
        Ok(())
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
        self.bind_impl(obj, prop, move |e| Some(f(e)), true)
    }

    /// [`Engine::bind_initial`] for a binding that can fail (a JS
    /// exception): `None` keeps the property's last value, as Qt does.
    pub fn bind_fallible(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        f: impl FnMut(&Engine) -> Option<Value> + 'static,
    ) -> Result<(), EngineError> {
        self.bind_impl(obj, prop, f, true)
    }

    /// [`Engine::bind_fallible`] that checks the property is writable
    /// (`Qt.binding` from JS).
    pub fn bind_fallible_checked(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        f: impl FnMut(&Engine) -> Option<Value> + 'static,
    ) -> Result<(), EngineError> {
        self.bind_impl(obj, prop, f, false)
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
        self.bind_free(obj, prop, move |e| Some(f(e)), false, Free::Always)
    }

    fn bind_impl(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        f: impl FnMut(&Engine) -> Option<Value> + 'static,
        internal: bool,
    ) -> Result<(), EngineError> {
        // A binding the creator installs gives the property its first
        // value, which no interceptor animates.
        let free = if internal { Free::First } else { Free::Never };
        self.bind_free(obj, prop, f, internal, free)
    }

    /// Installs `f` as the binding of `prop`. The creator's closure goes
    /// into the graph's box with the wrapper around it: one allocation per
    /// binding, where a box of its own made it two (B26).
    pub(crate) fn bind_free<F>(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        f: F,
        internal: bool,
        free: Free,
    ) -> Result<(), EngineError>
    where
        F: FnMut(&Engine) -> Option<Value> + 'static,
    {
        let to = self.redirect(obj, prop);
        if to != (obj, prop) {
            let f = self.traced_as_alias(obj, prop, to, Box::new(f));
            return self.bind_redirected(obj, prop, to, f, internal, free);
        }
        self.bind_redirected(obj, prop, to, f, internal, free)
    }

    fn bind_redirected<F>(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        to: (ObjectId, PropIndex),
        f: F,
        internal: bool,
        free: Free,
    ) -> Result<(), EngineError>
    where
        F: FnMut(&Engine) -> Option<Value> + 'static,
    {
        if let Some(target) = self.alias_member(obj, prop) {
            return self.bind_member(target, Box::new(f), internal, free);
        }
        let (obj, prop) = to;
        let (n, ty, last) = self.binding_target(obj, prop, internal)?;
        let wrapper = self.binding_wrapper(obj, prop, ty, last, f, free);
        self.inner.graph.bind(n, wrapper)?;
        self.binding_installed(n, obj, prop)?;
        self.tell_given(obj, prop);
        Ok(())
    }

    /// Runs the type's [`GivenHook`](crate::meta::GivenHook) for `prop`,
    /// if it has one, now that a binding or an override is in place (a
    /// value written is told in [`write_impl`](Self::write_impl)).
    fn tell_given(&self, obj: ObjectId, prop: PropIndex) {
        let hook = {
            let store = self.inner.store.borrow();
            store
                .objects
                .get(obj)
                .and_then(|h| h.meta.property(prop))
                .and_then(|p| p.given)
        };
        if let Some(hook) = hook {
            self.untrack(|| hook(&Cx::new(self, obj), prop));
        }
    }

    /// The node a binding of `prop` goes on, the type it gives and what it
    /// keeps when it fails.
    fn binding_target(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        internal: bool,
    ) -> Result<(NodeId, TypeTag, Value), EngineError> {
        self.drop_maker(obj, prop);
        let ty = {
            let store = self.inner.store.borrow();
            let h = store
                .objects
                .get(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            self.check_writable(h, prop, internal)?.clone()
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
        Ok((n, ty, last.unwrap_or_default()))
    }

    fn binding_installed(
        &self,
        n: NodeId,
        obj: ObjectId,
        prop: PropIndex,
    ) -> Result<(), EngineError> {
        if self.inner.eager_bindings.get() && !self.in_deferred_contents(obj) {
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
                self.inner.graph.set_eager(n, true)?;
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
    /// cache. What it keeps between runs is [`Wrapped`], which is not
    /// generic: only the call of `f` is made once per closure type.
    fn binding_wrapper<F>(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        ty: TypeTag,
        last: Value,
        mut f: F,
        free: Free,
    ) -> impl FnMut(&Graph<Value>) -> Value + 'static
    where
        F: FnMut(&Engine) -> Option<Value> + 'static,
    {
        let mut w = Wrapped {
            weak: Rc::downgrade(&self.inner),
            obj,
            prop,
            ty,
            last,
            free,
            reset: self.reset_of(obj, prop),
            slot: None,
            sought: false,
        };
        move |_g| w.run(&mut f)
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
                None if !h.read_without_node(prop) => {
                    let change = p.change;
                    drop(store);
                    self.tell_change(obj, prop, change);
                    return Ok(());
                }
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
        let look = |obj: ObjectId, prop: PropIndex, alias: bool| {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj)?;
            // Nearly no property asked is an alias: the look that says so
            // is the one that answers.
            if alias || h.meta.property(prop).is_some_and(|p| !p.is_alias()) {
                Some(Some((h.node(prop), h.is_assigned(prop))))
            } else {
                Some(None)
            }
        };
        let (node, assigned) = match look(obj, prop, false)? {
            Some(found) => found,
            None => {
                let (obj, prop) = self.redirect(obj, prop);
                look(obj, prop, true)??
            }
        };
        match node {
            Some(n) => self.inner.graph.provenance(n),
            None if assigned => Some(Provenance::Assigned),
            None => Some(Provenance::Default),
        }
    }

    /// The bits of `wanted` whose property in `props` an author gave a
    /// value: assigned or bound. Bit `i` stands for `props[i]`; a bit past
    /// the end of `props` is never set. One look at the object for all of
    /// them: a palette asks this of up to 84 colours, on the object and on
    /// every ancestor, each time it resolves.
    pub fn authored(&self, obj: ObjectId, props: &[PropIndex], wanted: u128) -> u128 {
        let given =
            |p: Option<Provenance>| matches!(p, Some(Provenance::Assigned | Provenance::Bound));
        let mut bits = 0u128;
        let mut aliases: SmallVec<[usize; 4]> = SmallVec::new();
        let mut nodes: SmallVec<[(usize, NodeId); 32]> = SmallVec::new();
        {
            let store = self.inner.store.borrow();
            let Some(h) = store.objects.get(obj) else {
                return 0;
            };
            let mut left = wanted;
            while left != 0 {
                let i = left.trailing_zeros() as usize;
                left &= left - 1;
                let Some(&prop) = props.get(i) else {
                    break;
                };
                if h.meta.property(prop).is_none_or(|p| p.is_alias()) {
                    aliases.push(i);
                } else if let Some(n) = h.node(prop) {
                    nodes.push((i, n));
                } else if h.is_assigned(prop) {
                    bits |= 1 << i;
                }
            }
        }
        self.inner.graph.provenance_each(nodes, |i, p| {
            bits |= u128::from(given(p)) << i;
        });
        for i in aliases {
            bits |= u128::from(given(self.provenance(obj, props[i]))) << i;
        }
        bits
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

    /// Whether the property's own layer, under every override, is a
    /// binding: what taking the overrides off gives back.
    pub fn base_is_bound(&self, obj: ObjectId, prop: PropIndex) -> bool {
        let (obj, prop) = self.redirect(obj, prop);
        let node = self
            .inner
            .store
            .borrow()
            .objects
            .get(obj)
            .and_then(|h| h.node(prop));
        node.is_some_and(|n| self.inner.graph.base_provenance(n) == Some(Provenance::Bound))
    }

    /// Whether the property's binding ran again for the change that is
    /// still spreading. Qt evaluates such a binding after what the change
    /// did natively, so its value has the last word over a native reset.
    pub fn ran_this_change(&self, obj: ObjectId, prop: PropIndex) -> bool {
        let (obj, prop) = self.redirect(obj, prop);
        let node = self
            .inner
            .store
            .borrow()
            .objects
            .get(obj)
            .and_then(|h| h.node(prop));
        node.is_some_and(|n| self.inner.graph.is_bound(n) && self.inner.graph.ran_this_flush(n))
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
            self.check_writable(h, prop, true)?.clone()
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
                let f = move |e: &Engine| Some(f(e));
                // An override's first value is put in place like a value
                // override is: no interceptor sees it.
                OverrideValue::binding(self.binding_wrapper(obj, prop, ty, last, f, Free::First))
            }
            OverrideContent::FallibleBinding(f) => {
                let last = g.untrack(|| g.try_get(n)).unwrap_or_default();
                OverrideValue::binding(self.binding_wrapper(obj, prop, ty, last, f, Free::First))
            }
        };
        let id = g.batch(|| {
            let id = g.push_override(n, value)?;
            self.sync_field(obj, prop, n);
            Ok::<_, EngineError>(id)
        })?;
        self.tell_given(obj, prop);
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

    /// Stops an effect made by [`Engine::effect`]: it runs no more.
    pub fn stop_effect(&self, effect: NodeId) {
        let _ = self.inner.graph.dispose(effect);
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

    /// [`Engine::effect_in`] whose first run is before this returns
    /// ([`Graph::effect_now`]): for an owner that needs what it reads now,
    /// in a batch or in another run, and would otherwise read it twice.
    ///
    /// [`Graph::effect_now`]: ljos_reactive::Graph::effect_now
    pub fn effect_now_in(
        &self,
        obj: ObjectId,
        mut f: impl FnMut(&Engine) + 'static,
    ) -> Result<NodeId, EngineError> {
        let scope = self.ensure_scope(obj)?;
        let weak = Rc::downgrade(&self.inner);
        Ok(self.inner.graph.effect_now(scope, move |_g| {
            run_effect(&weak, &mut |e: &Engine| {
                if !e.destruction_told(obj) {
                    f(e);
                }
            })
        })?)
    }

    /// [`Engine::effect_in`] that, once `f` has run, is told of a change of
    /// what it read instead of running again ([`Graph::effect_told`]): for
    /// an owner that reads the values itself when it next has use for them.
    ///
    /// [`Graph::effect_told`]: ljos_reactive::Graph::effect_told
    pub fn effect_told_in(
        &self,
        obj: ObjectId,
        mut f: impl FnMut(&Engine) + 'static,
        told: impl Fn() + 'static,
    ) -> Result<NodeId, EngineError> {
        let scope = self.ensure_scope(obj)?;
        let weak = Rc::downgrade(&self.inner);
        let g = &self.inner.graph;
        Ok(g.effect_told(
            scope,
            move |_g| {
                if let Some(inner) = weak.upgrade() {
                    let engine = Engine::from_inner(inner);
                    let _t = Tracking::enter(&engine.inner.tracking);
                    if !engine.destruction_told(obj) {
                        f(&engine);
                    }
                }
            },
            told,
        )?)
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
        Ok(self
            .inner
            .graph
            .watch(scope, n, move |_g, v| heard(&weak, v, &f))?)
    }

    fn effect_scoped(
        &self,
        scope: ScopeId,
        mut f: impl FnMut(&Engine) + 'static,
    ) -> Result<NodeId, EngineError> {
        let weak = Rc::downgrade(&self.inner);
        let g = &self.inner.graph;
        Ok(g.effect(scope, move |_g| run_effect(&weak, &mut f))?)
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
            // Which parameters take data, and which of those a copy.
            let mut data = (0u64, 0u64);
            for (i, (_, t)) in m.params.iter().enumerate().take(64) {
                match t {
                    TypeTag::Data => data.0 |= 1 << i,
                    TypeTag::Copied => {
                        data.0 |= 1 << i;
                        data.1 |= 1 << i;
                    }
                    _ => {}
                }
            }
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
        // What a script's call gave by copy has crossed as data already.
        let (data, copied) = data;
        let taken = |i: usize| i < 64 && data >> i & 1 == 1;
        let args = if args
            .iter()
            .enumerate()
            .any(|(i, a)| taken(i) && crate::js::holds_js(a))
        {
            std::borrow::Cow::Owned(
                args.iter()
                    .enumerate()
                    .map(|(i, a)| match taken(i) {
                        true if copied >> i & 1 == 1 => self.copy_of(a),
                        true => self.deep_copy(a),
                        false => self.plain(a),
                    })
                    .collect(),
            )
        } else {
            self.plain_args(args)
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
    /// How many objects are completing right now, the one asking
    /// included: above one, the object asking is made by another's
    /// completion (a Repeater's item, a Loader's), after everything
    /// written around that one was set.
    pub fn completing_depth(&self) -> u32 {
        self.inner.completing.get()
    }

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
        let depth = &self.inner.completing;
        depth.set(depth.get() + 1);
        let run = hooks
            .into_iter()
            .try_for_each(|f| self.guarded_result(obj, || f(&cx)));
        depth.set(depth.get() - 1);
        run?;
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
        // Called unborrowed: a listener may attach something itself. Most
        // are for one type, so only a few are picked out.
        let listeners: SmallVec<[AttachedListener; 4]> = {
            let store = self.inner.store.borrow();
            let meta = &store.objects[a].meta;
            self.inner
                .attached_listeners
                .borrow()
                .iter()
                .filter(|(ty, _)| ty.is_none_or(|t| meta.inherits(t)))
                .map(|(_, l)| Rc::clone(l))
                .collect()
        };
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
                .retain(|(_, l)| !done.iter().any(|d| Rc::ptr_eq(d, l)));
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
        self.inner
            .attached_listeners
            .borrow_mut()
            .push((None, Rc::new(f)));
    }

    /// [`Engine::on_attached_created`] for attached objects of type `T`, or
    /// one derived from it, only: an item may have many attached objects,
    /// and the listeners for the other types are not called for it.
    pub fn on_attached_created_of<T: QmlType>(
        &self,
        f: impl Fn(&Engine, ObjectId, ObjectId) -> bool + 'static,
    ) {
        self.inner
            .attached_listeners
            .borrow_mut()
            .push((Some(T::static_meta()), Rc::new(f)));
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
        crate::lookups::name();
        if let Some(c) = self.inner.contexts.borrow_mut().get_mut(ctx) {
            c.ids.insert(Atom::intern(name), obj);
        }
    }

    /// Takes `name` out of `ctx` while it names `obj`, so that the name is
    /// found further out, as a context that never had it finds it.
    pub(crate) fn unset_context_id(&self, ctx: ContextId, name: &str, obj: ObjectId) {
        crate::lookups::name();
        if let Some(c) = self.inner.contexts.borrow_mut().get_mut(ctx)
            && c.id(name) == Some(obj)
        {
            c.note_ids();
            c.ids.remove(name);
        }
    }

    /// Lets `ctx` find the ids of `inst`, the instance it is made for, by
    /// their slots.
    pub(crate) fn set_context_slots(&self, ctx: ContextId, inst: &Rc<crate::incubator::Instance>) {
        if let Some(c) = self.inner.contexts.borrow_mut().get_mut(ctx) {
            c.slots = Some(crate::context::IdSlots(inst.clone()));
        }
    }

    /// Writes every context's ids down one by one; see
    /// [`Context::note_ids`].
    pub(crate) fn note_context_ids(&self) {
        for (_, c) in self.inner.contexts.borrow_mut().iter_mut() {
            c.note_ids();
        }
    }

    /// Resolves an id from `ctx` outwards. An id whose object is gone
    /// resolves to `None` (`null` in QML).
    pub fn resolve_id(&self, ctx: ContextId, name: &str) -> Option<ObjectId> {
        crate::lookups::name();
        let found = {
            let contexts = self.inner.contexts.borrow();
            let mut cur = Some(ctx);
            let mut found = None;
            while let Some(c) = cur.and_then(|c| contexts.get(c)) {
                if let Some(o) = c.id(name) {
                    found = Some(o);
                    break;
                }
                cur = c.parent;
            }
            found
        };
        found.filter(|o| self.is_alive(*o))
    }

    /// See [`Context::root`].
    pub(crate) fn set_context_root(&self, ctx: ContextId, obj: ObjectId) {
        if let Some(c) = self.inner.contexts.borrow_mut().get_mut(ctx) {
            c.root = Some(obj);
        }
    }

    pub fn set_context_object(&self, ctx: ContextId, obj: Option<ObjectId>) {
        if let Some(c) = self.inner.contexts.borrow_mut().get_mut(ctx) {
            c.context_object = obj;
        }
    }

    /// Puts `name` in scope for everything created in `ctx` and its
    /// children (`setContextProperty`).
    pub fn set_context_property(&self, ctx: ContextId, name: &str, v: Value) {
        crate::lookups::name();
        if let Some(c) = self.inner.contexts.borrow_mut().get_mut(ctx) {
            c.props.insert(Atom::intern(name), v);
        }
    }

    /// A context property, looked up from `ctx` outwards: what was set by
    /// name, then the properties of the context's object (a delegate's
    /// row: `index`, `model`, the roles).
    pub fn context_property(&self, ctx: ContextId, name: &str) -> Option<Value> {
        crate::lookups::name();
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
        let p = self.find_property(obj, name)?;
        self.try_read(obj, p).ok()
    }

    /// A name the compiler could not place, looked up from `ctx` outwards
    /// as Qt does at run time: the ids of each context, then its context
    /// properties, the members of its root and of its context object. An
    /// inline component reads the ids of the file that uses it this way,
    /// and a file the members of the file it is made in.
    pub(crate) fn context_name(&self, ctx: ContextId, name: &str) -> Option<Value> {
        crate::lookups::name();
        let mut cur = Some(ctx);
        let found = loop {
            let objects = {
                let contexts = self.inner.contexts.borrow();
                let c = contexts.get(cur?)?;
                if let Some(o) = c.id(name) {
                    break o;
                }
                if let Some(v) = c.props.get(name) {
                    return Some(v.clone());
                }
                cur = c.parent;
                [c.root, c.context_object]
            };
            for o in objects.into_iter().flatten() {
                if let Some(v) = self.context_object_property(o, name) {
                    return Some(v);
                }
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
    /// given outright (or, with `ids`, an id or a member of a context's
    /// root). As [`Engine::context_property`] and, with `ids`,
    /// [`Engine::context_name`] look.
    pub(crate) fn context_holder(
        &self,
        ctx: ContextId,
        name: &str,
        ids: bool,
    ) -> Option<Result<ObjectId, Value>> {
        crate::lookups::name();
        let mut cur = Some(ctx);
        while let Some(id) = cur {
            let objects = {
                let contexts = self.inner.contexts.borrow();
                let c = contexts.get(id)?;
                if ids && let Some(o) = c.id(name) {
                    return Some(Err(if self.is_alive(o) {
                        Value::Object(o)
                    } else {
                        Value::Null
                    }));
                }
                if let Some(v) = c.props.get(name) {
                    return Some(Err(v.clone()));
                }
                cur = c.parent;
                [c.root.filter(|_| ids), c.context_object]
            };
            if let Some(o) = objects
                .into_iter()
                .flatten()
                .find(|o| self.property_index(*o, name).is_ok())
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

    /// The type Qt names after `QML ` in an object's own warning: the
    /// file or inline component `obj` is the root of, else the nearest
    /// such type it derives from (`MyCol {}` warns as `QML MyCol:`), else
    /// `builtin`, the native type the warning is about.
    pub fn warning_type_name(&self, obj: ObjectId, builtin: &str) -> String {
        let mut meta = self.meta_of(obj);
        while let Some(m) = meta {
            if matches!(m.declared, Some(Declared::File | Declared::Inline)) {
                return m.name.to_string();
            }
            meta = m.base.clone();
        }
        builtin.to_owned()
    }
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

/// Where [`Engine::plain_number`] puts a write.
enum PlainNumber {
    Node(NodeId),
    Field,
}

/// What a native field's setter makes of a write.
pub(crate) enum Filtered {
    /// What to store, or nothing if the setter refused it.
    Done(Option<Value>),
    /// A setter that gets a `Cx`, still to run with the value: it may write
    /// other properties, so it cannot run under the store's borrow.
    WithCx(crate::meta::CxWrite, Value),
}

/// A property's node, `Err` if a reader follows it without one, and the
/// write hook a model row has.
fn node_and_hook(
    h: &ObjectHeader,
    prop: PropIndex,
    internal: bool,
) -> (Result<Option<NodeId>, ()>, Option<crate::object::WriteHook>) {
    let hook = match &h.ext {
        Some(ext) if !internal => ext.write_hook.clone(),
        _ => None,
    };
    let node = match h.node(prop) {
        None if h.read_without_node(prop) => Err(()),
        n => Ok(n),
    };
    (node, hook)
}

/// What a write of `v` to a native field stores: `v`, what the field's
/// [`WriteFilter`](crate::meta::WriteFilter) makes of it, or nothing if
/// the filter refuses it.
fn filter_access(
    native: &dyn QmlObject,
    access: &PropertyAccess,
    v: Value,
) -> Result<Filtered, ConvertError> {
    match *access {
        PropertyAccess::Field {
            owner,
            write: Some(write),
            ..
        } => match (write, find_native(native, owner)) {
            (WriteFilter::Native(write), Some(o)) => Ok(Filtered::Done(write(o, &v)?)),
            (WriteFilter::Cx(write), Some(_)) => Ok(Filtered::WithCx(write, v)),
            (_, None) => Ok(Filtered::Done(Some(v))),
        },
        PropertyAccess::Group {
            owner,
            get,
            meta,
            leaf,
            ..
        } => match find_native(native, owner) {
            Some(o) => filter_access(get(o), &meta.properties[leaf.0 as usize].access, v),
            None => Ok(Filtered::Done(Some(v))),
        },
        _ => Ok(Filtered::Done(Some(v))),
    }
}

pub(crate) fn filter_in(
    h: &ObjectHeader,
    prop: PropIndex,
    v: Value,
) -> Result<Filtered, ConvertError> {
    match h.meta.property(prop) {
        Some(p) => filter_access(&*h.native, &p.access, v),
        None => Ok(Filtered::Done(Some(v))),
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

/// See [`Engine::write_look`].
enum WriteLook {
    Plain,
    Alias,
    Maker,
}

/// The property's [`ChangeHook`](crate::ChangeHook), told by
/// [`Engine::tell_change`] of a change to a field without a node.
pub(crate) fn change_hook(h: &ObjectHeader, prop: PropIndex) -> Option<crate::ChangeHook> {
    h.meta.property(prop).and_then(|p| p.change)
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

/// What a watch does when its property changes, for every handler: the
/// handler is the one part that differs.
#[inline(never)]
fn heard(weak: &Weak<Inner>, v: &Value, f: &dyn Fn(&Engine, &Value)) {
    if let Some(inner) = weak.upgrade() {
        let engine = Engine::from_inner(inner);
        let _t = Tracking::pause(&engine.inner.tracking);
        f(&engine, v);
    }
}

/// What an effect does when it runs, for every effect.
#[inline(never)]
fn run_effect(weak: &Weak<Inner>, f: &mut dyn FnMut(&Engine)) {
    if let Some(inner) = weak.upgrade() {
        let engine = Engine::from_inner(inner);
        let _t = Tracking::enter(&engine.inner.tracking);
        f(&engine);
    }
}

/// The error of a borrow as the wrong type: built out of line, as it is
/// in every typed borrow and taken by almost none.
#[cold]
#[inline(never)]
fn wrong_type(have: &Atom, expected: &MetaObject) -> EngineError {
    EngineError::WrongType {
        ty: have.to_string(),
        expected: expected.name.to_string(),
    }
}

/// Takes the one `id` out of `list`, keeping the order of the rest.
fn take_out(list: &mut Vec<ObjectId>, id: ObjectId) {
    if let Some(i) = list.iter().position(|c| *c == id) {
        list.remove(i);
    }
}

/// What a script reads of a property as it is held: no list, record or
/// object to be made a face of first.
fn primitive(v: &Value) -> bool {
    matches!(
        v,
        Value::Int(_) | Value::Real(_) | Value::Bool(_) | Value::String(_)
    )
}

/// What a binding keeps between runs (see [`Engine::binding_wrapper`]).
struct Wrapped {
    weak: Weak<Inner>,
    obj: ObjectId,
    prop: PropIndex,
    ty: TypeTag,
    last: Value,
    free: Free,
    reset: Option<crate::meta::Reset>,
    /// Where a number property's field is, found at the first run: a
    /// number it gives then goes straight into it, as an animation's
    /// steps do. It is looked for again whenever it stops fitting.
    slot: Option<crate::NumberSlot>,
    sought: bool,
}

impl Wrapped {
    fn run(&mut self, f: &mut dyn FnMut(&Engine) -> Option<Value>) -> Value {
        let Wrapped {
            ref weak,
            obj,
            prop,
            ref ty,
            ref mut last,
            ref mut free,
            reset,
            ref mut slot,
            ref mut sought,
        } = *self;
        let Some(inner) = weak.upgrade() else {
            return last.clone();
        };
        let engine = Engine::from_inner(inner);
        if engine.destruction_told(obj) {
            return last.clone();
        }
        // While a trace follows bindings each run is told: what it
        // read that changed, how long it took and what it gave.
        // Otherwise this is one look at a flag.
        let _timed = engine.inner.timing.binding();
        let _profiled = engine.profile_binding(obj, prop);
        let _journaled = engine.journal_binding(obj, prop);
        let followed = if engine.inner.traces.follows_bindings() {
            engine
                .binding_runs(obj, prop)
                .map(|run| (run, last.clone()))
        } else {
            None
        };
        let mut run = || -> Value {
            let unseen = *free != Free::Never;
            if *free == Free::First {
                *free = Free::Never;
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
            if !*sought {
                *sought = true;
                *slot = engine.number_slot(obj, prop);
            }
            // A number needs none of the conversions and has no filter.
            if let Some(s) = *slot
                && let Some(n) = s.number(&v)
            {
                *last = n;
                if !unseen && let Some(shown) = engine.intercept_binding(obj, prop, last) {
                    return shown;
                }
                if !engine.store_number(&s, last) {
                    *slot = None;
                    *sought = false;
                    engine.store_value(obj, prop, last);
                }
                return last.clone();
            }
            let v = engine.for_slot(obj, prop, v);
            let coerced = {
                let store = engine.inner.store.borrow();
                coerce_for(&store, Some(obj), ty, &v).and_then(|v| match store.objects.get(obj) {
                    Some(h) => Ok(filter_in(h, prop, v)?),
                    None => Ok(Filtered::Done(Some(v))),
                })
            };
            let coerced = match coerced {
                Ok(Filtered::Done(v)) => Ok(v),
                Ok(setter) => engine.finish_filter(obj, prop, setter),
                Err(e) => Err(e),
            };
            match coerced {
                // The type refused it: the property keeps what it holds.
                Ok(None) => {}
                Ok(Some(v)) => *last = v,
                Err(e) => {
                    let e = engine.refusal(&v, &e);
                    // Qt's message is the binding's place and the error; a
                    // binding made at run time has no place to give.
                    let msg = match engine.binding_place(obj, prop) {
                        Some(at) => format!("{at}: {e}"),
                        None => {
                            let (t, p) = engine.describe(obj, prop);
                            format!("QML {t}: {e} (property \"{p}\")")
                        }
                    };
                    engine.warn_of_binding(obj, prop, msg);
                }
            }
            if !unseen && let Some(shown) = engine.intercept_binding(obj, prop, last) {
                return shown;
            }
            engine.store_value(obj, prop, last);
            last.clone()
        };
        let v = run();
        if let Some((run, before)) = followed {
            engine.binding_ran(run, &before, &v);
        }
        v
    }
}
