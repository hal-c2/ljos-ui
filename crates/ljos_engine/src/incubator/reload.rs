//! Dev hot reload on the engine's side (ARCHITECTURE.md §12.1).
//!
//! A [`ReloadPatch`] is applied by making the application's roots again
//! from the patched files and carrying across what the old objects held,
//! object by object. The old roots go away only once every new one stands
//! ("the new root is created before the old one is deleted"), so a patch
//! applies whole or not at all: when a root cannot be made, what was made
//! is destroyed, the files are put back and the old objects never notice.
//!
//! **Identity.** While [`Engine::set_reload_tracking`] is on, every object
//! made from QML gets a path: where it is, in terms that survive an edit.
//! Inside one component instance that is the object's `id`, else its
//! `objectName` or its type and place among its siblings below its
//! parent's path (the `KeySeg` order of preference). A component used as
//! a type continues the path of the object that uses it; a delegate, a
//! `Loader` item or anything else made later continues the path of its
//! owner with its component and, for a delegate, its row.
//!
//! An object of the new tree is **kept** when the old tree had an object
//! of the same path made from the same types, **remounted** when the path
//! is the same and the types are not (or its file says `pragma
//! LjosReload: reset`), and new otherwise. Everything below a remounted
//! object is remounted with it.
//!
//! **What a kept object carries** (and nothing else: bindings and handler
//! bodies are always the new file's):
//!
//! - what a script assigned (a write from JavaScript or
//!   [`Engine::write_by_name`]), unless the new file gives the property
//!   another declaration than the old one did;
//! - the properties registered with [`Engine::register_reload_state`]
//!   (`Timer.running`, a `Loader`'s `source`, a button's `checked`), under
//!   the same condition and only where the new file binds nothing;
//! - whatever its native type carries itself through [`Migrate`]
//!   (`ListModel`'s rows).
//!
//! A kept object does not run `Component.onCompleted` again; it runs
//! `Component.onReloaded`.

use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use ljos_ir::{CompiledFile, KeySeg, ObjIdx, ObjRef, ObjectKey, ReloadOp, ReloadPatch};
use ljos_js::Bytecode;

pub(super) mod in_place;

use super::{Creation, Initial, Instance, Template};
use crate::atom::Atom;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{ContextId, ObjectId, PropIndex};
use crate::loader::{ComponentSource, LoadedFile};
use crate::meta::{MetaObject, MetaRef, PropertyFlags};
use crate::object::QmlType;
use crate::types::TypeTag;
use crate::value::Value;
use crate::{Provenance, WriteKind};

/// State of a native type that a reload carries from the old object to
/// the one that takes its place (ARCHITECTURE.md §12.1, "Rust state").
///
/// Properties need none of this: a script's assignments and the
/// properties named with [`Engine::register_reload_state`] are carried by
/// name. Implement `Migrate` for what a type keeps outside its properties
/// (a model's rows, an editor's undo stack, a view's stack of pages) and
/// register it with [`Engine::register_migrate`].
///
/// ```
/// use ljos_engine::{Engine, Migrate, ObjectId, QmlObject, QtObject};
///
/// #[derive(Debug, Default, QmlObject)]
/// #[qml(module = "Example", name = "Tally", extends = QtObject)]
/// pub struct Tally {
///     #[qml(base)]
///     object: QtObject,
///     #[qml(skip)]
///     marks: Vec<u32>,
/// }
///
/// impl Migrate for Tally {
///     const VERSION: u32 = 1;
///     type State = Vec<u32>;
///
///     fn snapshot(engine: &Engine, obj: ObjectId) -> Option<Vec<u32>> {
///         engine.with::<Tally, _>(obj, |t| t.marks.clone()).ok()
///     }
///
///     fn migrate(engine: &Engine, obj: ObjectId, _from: u32, marks: Vec<u32>) -> Result<(), String> {
///         engine
///             .with_mut::<Tally, _>(obj, |t| t.marks = marks)
///             .map_err(|e| e.to_string())
///     }
/// }
///
/// let engine = Engine::new();
/// engine.register_migrate::<Tally>();
/// ```
pub trait Migrate: QmlType {
    /// Bumped when [`Migrate::State`] changes its meaning. `migrate` is
    /// told the version the state was taken at.
    const VERSION: u32;
    type State: 'static;

    /// What `obj` holds that is worth keeping; `None` for nothing.
    fn snapshot(engine: &Engine, obj: ObjectId) -> Option<Self::State>;

    /// Gives `obj`, freshly made from the new file, what the old object
    /// held. On `Err` the object stays as the file declares it and counts
    /// as new; the failure is logged.
    fn migrate(
        engine: &Engine,
        obj: ObjectId,
        from_version: u32,
        state: Self::State,
    ) -> Result<(), String>;
}

type SnapshotFn = dyn Fn(&Engine, ObjectId) -> Option<Box<dyn Any>>;
type MigrateFn = dyn Fn(&Engine, ObjectId, Box<dyn Any>) -> Result<(), String>;

pub(super) struct Migrator {
    meta: &'static MetaObject,
    snapshot: Box<SnapshotFn>,
    migrate: Box<MigrateFn>,
}

/// Where a reload went wrong, for a developer to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReloadFault {
    /// The file's path from the project's root, with `.qml`; empty when
    /// the fault is not in one file.
    pub file: String,
    /// 1-based; 0 when unknown.
    pub line: u32,
    pub column: u32,
    pub message: String,
}

impl std::fmt::Display for ReloadFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.file.is_empty(), self.line) {
            (true, _) => f.write_str(&self.message),
            (false, 0) => write!(f, "{}: {}", self.file, self.message),
            (false, _) => write!(
                f,
                "{}:{}:{}: {}",
                self.file, self.line, self.column, self.message
            ),
        }
    }
}

/// Why [`Engine::reload`] applied nothing.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ReloadError {
    #[error("reload tracking is off: Engine::set_reload_tracking(true) comes before the objects")]
    NotTracking,
    #[error("the engine has no component source to patch")]
    NoSource,
    /// The patch was computed against another generation than the engine
    /// is at; its ops would name the wrong objects.
    #[error("the patch is for generation {patch}, the engine is at {engine}")]
    Stale { patch: u64, engine: u64 },
    /// A root that was not made by [`Engine::create_from_url`] while
    /// tracking was on.
    #[error("object {0:?} is not a root this engine can make again")]
    UnknownRoot(ObjectId),
    /// The new files do not make the application. The old one is intact.
    #[error("{0}")]
    Failed(ReloadFault),
}

/// What [`Engine::reload`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReloadOutcome {
    pub generation: u64,
    /// (the root that is gone, the root that took its place), in the
    /// order the roots were given.
    pub roots: Vec<(ObjectId, ObjectId)>,
    /// Objects that went on with what they held.
    pub kept: usize,
    /// Objects the old tree had nothing for.
    pub fresh: usize,
    /// Objects made anew although the old tree had one at their place.
    pub remounted: usize,
}

struct Info {
    path: Rc<str>,
    /// The types the object is made from, outermost first.
    print: Rc<str>,
    /// Which making of the tree it belongs to: an old and a new object
    /// share a path while a reload runs.
    tree: u64,
}

/// An object a script made (`createObject`) under an object of the tree,
/// which nothing in the new tree makes: a reload makes it again under the
/// object that goes on for its parent.
struct Scripted {
    /// In the order they were made, which is the order they are made
    /// again: one may be the parent of the next.
    serial: u64,
    component: ObjectId,
    template: Rc<Template>,
    parent: ObjectId,
    props: Vec<(String, Value)>,
}

/// How a root was made, to make it again.
#[derive(Clone)]
struct Origin {
    url: String,
    parent: Option<ObjectId>,
    props: Vec<(String, Value)>,
    /// See [`Engine::create_from_url_in`].
    scope: Option<ObjectId>,
}

#[derive(Clone)]
struct Carried {
    name: Atom,
    ty: TypeTag,
    value: Value,
    /// Assigned by a script, which outranks a binding of the new file.
    script: bool,
}

/// What a reload carries from an object to the one that goes on for it.
pub(super) struct Taken {
    values: Vec<Carried>,
    /// (index of the migrator, its state).
    native: Vec<(usize, Box<dyn Any>)>,
}

struct Snap {
    old: ObjectId,
    print: Rc<str>,
    values: Vec<Carried>,
    /// (index of the migrator, its state).
    native: Vec<(usize, Box<dyn Any>)>,
    /// The new object that goes on for the old one. One on its way out (a
    /// row a view made again) leaves the place to the next one made.
    taker: Option<ObjectId>,
}

/// A reload from the moment the old tree is read until the host says the
/// new one has settled ([`Engine::finish_reload`]): objects made in
/// between (delegates, `Loader` items) are still matched to the old ones.
#[derive(Default)]
struct Pending {
    snaps: HashMap<Rc<str>, Snap>,
    /// (file key, object) to the properties whose declaration changed.
    changed: HashMap<(String, ObjectKey), Vec<String>>,
    /// Objects that had an object added, removed, moved or edited
    /// directly inside them.
    children_changed: HashSet<(String, ObjectKey)>,
    changed_files: HashSet<String>,
    /// New object to its path.
    kept: HashMap<ObjectId, Rc<str>>,
    /// Old object to the new one that goes on for it.
    counterpart: HashMap<ObjectId, ObjectId>,
    remounted: HashSet<ObjectId>,
    fresh: usize,
    /// The roots being made again: what they own goes with them.
    old_roots: HashSet<ObjectId>,
}

struct Overlay {
    installed: Rc<dyn ComponentSource>,
    base: Rc<dyn ComponentSource>,
    files: HashMap<String, Option<Arc<CompiledFile>>>,
}

/// The files of the application with every reload so far on top.
struct PatchedSource {
    base: Rc<dyn ComponentSource>,
    files: HashMap<String, Option<Arc<CompiledFile>>>,
}

impl ComponentSource for PatchedSource {
    fn load(&self, key: &str) -> Option<Arc<CompiledFile>> {
        match self.files.get(key) {
            Some(f) => f.clone(),
            None => self.base.load(key),
        }
    }

    fn failure(&self, key: &str) -> Option<String> {
        // A file of the patch compiled, or is gone.
        if self.files.contains_key(key) {
            None
        } else {
            self.base.failure(key)
        }
    }

    fn url(&self, key: &str) -> String {
        self.base.url(key)
    }

    fn key(&self, url: &str) -> String {
        self.base.key(url)
    }

    fn bytecode(&self, key: &str) -> Option<Bytecode> {
        // Bytecode of the build is for the build's source, not the edit's.
        if self.files.contains_key(key) {
            None
        } else {
            self.base.bytecode(key)
        }
    }
}

type MetaCache = rustc_hash::FxHashMap<(u32, u32), MetaRef>;

/// What [`Engine::reload`] puts back when the new files do not work.
struct SavedLoader {
    source: Rc<dyn ComponentSource>,
    files: std::collections::HashMap<String, Rc<LoadedFile>>,
    metas: Vec<(Rc<LoadedFile>, MetaCache)>,
    overlay: Option<Overlay>,
}

/// The names of a replaced type's members, by index.
pub(crate) struct OldMembers {
    pub(crate) props: Vec<Atom>,
    pub(crate) methods: Vec<Atom>,
}

#[derive(Default)]
pub(crate) struct ReloadState {
    pub(crate) tracking: Cell<bool>,
    /// Whether the engine's own reload state is registered: other crates
    /// register migrators of their own before tracking is turned on.
    builtins: Cell<bool>,
    /// An object a reload replaced that a script may still hold, to the
    /// one that went on for it and the old type, which names the members
    /// a script asks for.
    forwards: RefCell<HashMap<ObjectId, (ObjectId, Rc<OldMembers>)>>,
    /// Whether any handler is muted at all: one read for the usual case.
    muting: Cell<bool>,
    muted_all: Cell<bool>,
    muted: RefCell<HashSet<ObjectId>>,
    generation: Cell<u64>,
    /// Counts reload attempts. JS modules are named with it, as a module
    /// name cannot be loaded twice and a module cannot be unloaded.
    pub(crate) epoch: Cell<u64>,
    tree: Cell<u64>,
    serial: Cell<u64>,
    /// Where to start looking for a free number for the next root a
    /// script makes at a path: every one below was taken when last
    /// looked. Forgotten whenever a place may have come free, so a game
    /// that makes hundreds of pieces does not count past all of them for
    /// each one.
    numbered: RefCell<HashMap<String, u32>>,
    info: RefCell<HashMap<ObjectId, Info>>,
    by_path: RefCell<HashMap<Rc<str>, ObjectId>>,
    /// What scripts assigned, by the object that holds the value.
    assigned: RefCell<HashMap<ObjectId, Vec<PropIndex>>>,
    origins: RefCell<HashMap<ObjectId, Origin>>,
    scripted: RefCell<HashMap<ObjectId, Scripted>>,
    /// The creation that starts next is a script's: its root is given a
    /// path of its own beside the ones made before it from the same
    /// component under the same parent.
    scripting: Cell<bool>,
    /// The path of the root made next: the one it replaces.
    forced: RefCell<Option<Rc<str>>>,
    /// The row a view is making a delegate for, where the delegate has
    /// no `index` in sight (it takes its roles as required properties).
    rows: RefCell<HashMap<ObjectId, i64>>,
    pending: RefCell<Option<Pending>>,
    last_error: RefCell<Option<ReloadFault>>,
    /// (type name or empty for any, property name).
    state_props: RefCell<Vec<(String, String)>>,
    migrators: RefCell<Vec<Rc<Migrator>>>,
    overlay: RefCell<Option<Overlay>>,
    /// Every component instance made while tracking, in the order made:
    /// where a reload that only gives objects other values finds them
    /// (in_place.rs). Held until their objects are gone and a sweep sees it.
    pub(crate) instances: RefCell<Vec<Rc<Instance>>>,
    /// `Component` objects given a template while tracking.
    templates: RefCell<Vec<ObjectId>>,
    /// The code of every binding, handler and method made while tracking,
    /// which a reload in place points at the file as it is now.
    slots: RefCell<Vec<Weak<in_place::CodeSlot>>>,
    /// The length of `instances`, `templates` and `slots` at which the dead ones
    /// are swept out next.
    sweep_at: Cell<usize>,
    /// The properties whoever made a root gave it, which outrank what its
    /// file says.
    given: RefCell<HashMap<ObjectId, Vec<String>>>,
    /// What a native type binds a property to when it is not given one,
    /// which a member taken away by a reload is bound to again.
    defaults: RefCell<HashMap<(ObjectId, PropIndex), DefaultBinding>>,
}

/// A native type's binding for a property it was not given.
pub(crate) type DefaultBinding = Rc<dyn Fn(&Engine) -> Value>;

// Paths and prints are made for every object a reload makes, so they are
// pushed piece by piece rather than formatted.
fn push_seg(out: &mut String, seg: &KeySeg) {
    match seg {
        KeySeg::Id(id) => {
            out.push('#');
            out.push_str(id);
        }
        KeySeg::Named(name) => {
            out.push('\'');
            out.push_str(name);
            out.push('\'');
        }
        KeySeg::ModelRow(row) => {
            let _ = write!(out, "[{row}]");
        }
        KeySeg::Typed { ty, nth } => {
            out.push_str(ty);
            let _ = write!(out, "[{nth}]");
        }
    }
}

fn push_type(out: &mut String, ty: &ljos_ir::TypeRef) {
    match ty {
        ljos_ir::TypeRef::Native { module, name } => {
            out.push_str("N:");
            out.push_str(module);
            out.push(':');
            out.push_str(name);
        }
        ljos_ir::TypeRef::Component { file, inline } => {
            out.push_str("C:");
            out.push_str(file);
            if let Some(i) = inline {
                out.push(':');
                out.push_str(i);
            }
        }
        ljos_ir::TypeRef::Unresolved(name) => {
            out.push_str("U:");
            out.push_str(name);
        }
    }
}

fn resets(file: &CompiledFile) -> bool {
    file.pragmas
        .iter()
        .any(|p| p.name == "LjosReload" && p.value.as_deref() == Some("reset"))
}

/// The objects of one creation with their paths and types.
#[derive(Default)]
struct Walk {
    /// Owners before what they own.
    order: Vec<ObjectId>,
    path: HashMap<ObjectId, String>,
    print: HashMap<ObjectId, String>,
    reset: HashSet<ObjectId>,
    seen: HashMap<String, u32>,
}

impl Walk {
    /// `prefix` is the path of the instance's root. Without `owns_root`
    /// the root is the object of another instance that uses this
    /// component as its type, and has its path from there.
    fn instance(&mut self, inst: &Rc<Instance>, prefix: &str, owns_root: bool) {
        let def = inst.file.ir.component(inst.comp);
        let reset = resets(&inst.file.ir);
        let mut local: Vec<String> = Vec::with_capacity(def.objects.len());
        for (i, od) in def.objects.iter().enumerate() {
            let id = inst.objects[i];
            let mut p = match (&od.key, od.parent) {
                (_, None) => prefix.to_owned(),
                (KeySeg::Id(name), _) => {
                    let mut p = String::with_capacity(prefix.len() + name.len() + 1);
                    p.push_str(prefix);
                    p.push('#');
                    p.push_str(name);
                    p
                }
                (seg, Some(parent)) => {
                    let above = &local[parent.index()];
                    let mut p = String::with_capacity(above.len() + 16);
                    p.push_str(above);
                    p.push('/');
                    push_seg(&mut p, seg);
                    p
                }
            };
            if i > 0 || owns_root {
                // Two siblings of one objectName: numbered, as the
                // compiler's diff numbers them.
                let n = match self.seen.get_mut(p.as_str()) {
                    Some(n) => {
                        *n += 1;
                        *n
                    }
                    None => {
                        self.seen.insert(p.clone(), 1);
                        1
                    }
                };
                if n > 1 {
                    let _ = write!(p, "~{n}");
                }
                self.order.push(id);
                self.path.insert(id, p.clone());
            }
            let print = self.print.entry(id).or_default();
            print.push_str(&inst.file.key);
            print.push('|');
            print.push_str(&def.id);
            print.push('|');
            push_type(print, &od.ty);
            print.push(';');
            if reset {
                self.reset.insert(id);
            }
            if let Some(Some(inner)) = inst.inner.get(i) {
                let mut below = p.clone();
                below.push('>');
                below.push_str(&inner.file.key);
                below.push('|');
                below.push_str(&inner.file.ir.component(inner.comp).id);
                self.instance(inner, &below, false);
            }
            local.push(p);
        }
    }
}

/// `v` for the new tree: an object of the old tree is the object that
/// goes on for it, or, in a list or record, `null` when nothing does. An
/// object the reload does not make again is itself. `None` when `v` is
/// an object of the old tree that nothing goes on for.
fn translate(
    v: Value,
    alive: &dyn Fn(ObjectId) -> bool,
    new_for: &dyn Fn(ObjectId) -> Option<Option<ObjectId>>,
) -> Option<Value> {
    match v {
        Value::Object(o) => match new_for(o) {
            Some(new) => new.map(Value::Object),
            None => alive(o).then_some(Value::Object(o)),
        },
        Value::List(items) if items.iter().any(holds_object) => Some(Value::List(Arc::new(
            items
                .iter()
                .map(|i| translate(i.clone(), alive, new_for).unwrap_or(Value::Null))
                .collect(),
        ))),
        Value::Map(fields) if fields.iter().any(|(_, v)| holds_object(v)) => {
            Some(Value::Map(Arc::new(
                fields
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            translate(v.clone(), alive, new_for).unwrap_or(Value::Null),
                        )
                    })
                    .collect(),
            )))
        }
        v => Some(v),
    }
}

fn holds_object(v: &Value) -> bool {
    match v {
        Value::Object(_) => true,
        Value::List(items) => items.iter().any(holds_object),
        Value::Map(fields) => fields.iter().any(|(_, v)| holds_object(v)),
        _ => false,
    }
}

impl Engine {
    /// Turns dev reload on or off. While it is on, the engine notes for
    /// every object made from QML where it stands and what scripts assign
    /// to it, which [`Engine::reload`] needs. It costs memory per object
    /// and nothing runs for it between events; a shipped application
    /// leaves it off. Objects made before it was turned on are unknown to
    /// a reload.
    pub fn set_reload_tracking(&self, on: bool) {
        let st = &self.inner.reload;
        st.tracking.set(on);
        if on && !st.builtins.replace(true) {
            self.register_reload_state("Timer", "running");
            self.register_migrate::<crate::listmodel::ListModel>();
        }
    }

    pub fn reload_tracking(&self) -> bool {
        self.inner.reload.tracking.get()
    }

    /// The generation of the last patch applied; 0 before any.
    pub fn reload_generation(&self) -> u64 {
        self.inner.reload.generation.get()
    }

    /// Names a property that holds what the user did rather than what the
    /// file says, so a reload carries it: `("Loader", "source")`,
    /// `("AbstractButton", "checked")`. `type_name` is the name of a type
    /// or of one it derives from; empty means any type with a property of
    /// that name. The value is carried when it was given (not the
    /// default), is not bound, and the reloaded file neither binds the
    /// property nor declares it differently than before.
    pub fn register_reload_state(&self, type_name: &str, property: &str) {
        let mut props = self.inner.reload.state_props.borrow_mut();
        let entry = (type_name.to_owned(), property.to_owned());
        if !props.contains(&entry) {
            props.push(entry);
        }
    }

    /// Lets `T` carry its own state across a reload ([`Migrate`]).
    pub fn register_migrate<T: Migrate>(&self) {
        let meta = T::static_meta();
        let mut all = self.inner.reload.migrators.borrow_mut();
        if all.iter().any(|m| std::ptr::eq(m.meta, meta)) {
            return;
        }
        all.push(Rc::new(Migrator {
            meta,
            snapshot: Box::new(|e, o| T::snapshot(e, o).map(|s| Box::new(s) as Box<dyn Any>)),
            migrate: Box::new(|e, o, state| match state.downcast::<T::State>() {
                Ok(s) => T::migrate(e, o, T::VERSION, *s),
                Err(_) => Err("the state is of another type".to_owned()),
            }),
        }));
    }

    /// The last reload that failed, until one succeeds: what a dev
    /// overlay shows.
    pub fn last_reload_error(&self) -> Option<ReloadFault> {
        self.inner.reload.last_error.borrow().clone()
    }

    /// Records a reload that failed before it reached the engine (the
    /// file did not compile), or clears the error.
    pub fn set_last_reload_error(&self, fault: Option<ReloadFault>) {
        *self.inner.reload.last_error.borrow_mut() = fault;
    }

    /// The stable path of an object made from QML while tracking is on:
    /// the same for the object that takes its place after a reload.
    pub fn reload_path(&self, obj: ObjectId) -> Option<String> {
        let info = self.inner.reload.info.borrow();
        info.get(&obj).map(|i| i.path.to_string())
    }

    /// The object that went on for `old` in the last reload, until
    /// [`Engine::finish_reload`]. `old` may be gone already.
    pub fn reload_counterpart(&self, old: ObjectId) -> Option<ObjectId> {
        let pending = self.inner.reload.pending.borrow();
        let new = pending.as_ref()?.counterpart.get(&old).copied()?;
        self.is_alive(new).then_some(new)
    }

    /// Ends the time in which newly made objects are matched to the old
    /// tree. Call it once the application has settled after
    /// [`Engine::reload`] (after the frame that follows it); the next
    /// reload does it otherwise.
    pub fn finish_reload(&self) {
        *self.inner.reload.pending.borrow_mut() = None;
    }

    /// Applies `patch` as one transaction: every root in `roots` is made
    /// again from the patched files, takes over what its old objects
    /// held, and replaces the old root, which is destroyed. `swap` is
    /// called with (old, new) for each root once all of them stand and
    /// before any old one goes: where the host moves whatever points at
    /// the root (a window's content, its own list of roots).
    ///
    /// On `Err` nothing has changed: the old objects, files and
    /// generation are as they were, and [`Engine::last_reload_error`]
    /// says why.
    ///
    /// Call it between frames, outside any batch, with roots made by
    /// [`Engine::create_from_url`] while tracking was on.
    pub fn reload(
        &self,
        patch: &ReloadPatch,
        roots: &[ObjectId],
        swap: &mut dyn FnMut(&Engine, ObjectId, ObjectId),
    ) -> Result<ReloadOutcome, ReloadError> {
        let st = &self.inner.reload;
        if !st.tracking.get() {
            return Err(ReloadError::NotTracking);
        }
        if patch.base != st.generation.get() {
            return Err(ReloadError::Stale {
                patch: patch.base,
                engine: st.generation.get(),
            });
        }
        let mut plan: Vec<(ObjectId, Origin, Rc<str>)> = Vec::with_capacity(roots.len());
        for &root in roots {
            let origin = st.origins.borrow().get(&root).cloned();
            let path = st.info.borrow().get(&root).map(|i| i.path.clone());
            match (origin, path) {
                (Some(o), Some(p)) if self.is_alive(root) => plan.push((root, o, p)),
                _ => return Err(ReloadError::UnknownRoot(root)),
            }
        }
        *st.pending.borrow_mut() = None;
        if let Some(done) = self.reload_in_place(patch, roots) {
            self.recount_modules();
            return Ok(done);
        }
        let mut phases = Phases::start();
        let mut pending = self.reload_snapshot(patch);
        pending.old_roots = plan.iter().map(|(root, ..)| *root).collect();
        phases.mark("snapshot");
        let old: HashSet<ObjectId> = st.info.borrow().keys().copied().collect();
        let saved = self.reload_patch_loader(patch, true)?;
        phases.mark("loader");
        st.epoch.set(st.epoch.get() + 1);
        st.tree.set(st.tree.get() + 1);
        st.numbered.borrow_mut().clear();
        *st.pending.borrow_mut() = Some(pending);
        // The old tree hears nothing of the new one being made beside it.
        *st.muted.borrow_mut() = old;
        st.muting.set(true);

        let mut made: Vec<(ObjectId, ObjectId)> = Vec::with_capacity(plan.len());
        let mut failure = None;
        for (old_root, origin, path) in &plan {
            *st.forced.borrow_mut() = Some(path.clone());
            // The window keeps its singletons.
            self.singleton_scope_adopt(*old_root);
            match self.create_from_url_in(
                &origin.url,
                origin.parent,
                origin.props.clone(),
                origin.scope,
            ) {
                Ok(new) => made.push((*old_root, new)),
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            }
        }
        st.forced.borrow_mut().take();
        self.singleton_scope_adopt_none();
        phases.mark("make");

        if let Some(e) = failure {
            // Nothing of the attempt is to be heard, also not that it ends.
            st.muted_all.set(true);
            for (_, new) in &made {
                self.destroy_now(*new);
            }
            st.muted_all.set(false);
            st.muting.set(false);
            st.muted.borrow_mut().clear();
            *st.pending.borrow_mut() = None;
            st.tree.set(st.tree.get() - 1);
            st.numbered.borrow_mut().clear();
            self.reload_restore_loader(saved);
            // The attempt put its own objects at the old ones' paths.
            let tree = st.tree.get();
            let mut by_path = st.by_path.borrow_mut();
            by_path.clear();
            for (id, i) in st.info.borrow().iter() {
                if i.tree == tree {
                    by_path.insert(i.path.clone(), *id);
                }
            }
            drop(by_path);
            let fault = self.reload_fault(&e);
            *st.last_error.borrow_mut() = Some(fault.clone());
            return Err(ReloadError::Failed(fault));
        }

        self.reload_remake_scripted();
        phases.mark("scripted");
        for (old_root, new) in &made {
            self.singleton_scope_moved(*old_root, *new);
            swap(self, *old_root, *new);
        }
        phases.mark("swap");
        // An object that goes on is not told that it is destroyed; one
        // that is gone from the file is.
        let going_on: HashSet<ObjectId> = st
            .pending
            .borrow()
            .as_ref()
            .map(|p| p.counterpart.keys().copied().collect())
            .unwrap_or_default();
        *st.muted.borrow_mut() = going_on;
        // Before they go: a binding on an object that outlives the tree
        // runs again as its objects are destroyed.
        self.reload_note_forwards();
        phases.mark("forwards");
        for (old_root, _) in &made {
            self.destroy_now(*old_root);
        }
        self.reload_sweep();
        phases.mark("destroy");
        st.muting.set(false);
        st.muted.borrow_mut().clear();
        st.generation.set(patch.generation);
        *st.last_error.borrow_mut() = None;
        drop(saved);
        // The replaced files' types are held by nothing now but the
        // prototypes made for them, which would pile up reload by reload.
        if let Some(js) = self.inner.js.borrow().clone() {
            js.release_unused_types();
        }
        phases.mark("types");
        self.recount_modules();
        phases.done();
        let pending = st.pending.borrow();
        let p = pending.as_ref();
        Ok(ReloadOutcome {
            generation: patch.generation,
            roots: made,
            kept: p.map_or(0, |p| p.kept.len()),
            fresh: p.map_or(0, |p| p.fresh),
            remounted: p.map_or(0, |p| p.remounted.len()),
        })
    }

    /// `createObject` and `incubateObject` of `component`: a reload makes
    /// what it makes under an object of the tree again. Its time is not the
    /// script's ([`Engine::native_for_script`]).
    pub(crate) fn create_for_script(
        &self,
        component: ObjectId,
        template: Rc<Template>,
        parent: Option<ObjectId>,
        props: Vec<(String, Value)>,
        how: Initial,
    ) -> Result<ObjectId, EngineError> {
        self.native_for_script(|| {
            self.create_for_script_now(component, template, parent, props, how)
        })
    }

    fn create_for_script_now(
        &self,
        component: ObjectId,
        template: Rc<Template>,
        parent: Option<ObjectId>,
        props: Vec<(String, Value)>,
        how: Initial,
    ) -> Result<ObjectId, EngineError> {
        let st = &self.inner.reload;
        let Some(parent) = parent.filter(|_| st.tracking.get()) else {
            return self.create_from_template_for_script(&template, parent, props, how);
        };
        st.scripting.set(true);
        let made =
            self.create_from_template_for_script(&template, Some(parent), props.clone(), how);
        st.scripting.set(false);
        if let Ok(obj) = made
            && self.is_alive(obj)
        {
            st.serial.set(st.serial.get() + 1);
            st.scripted.borrow_mut().insert(
                obj,
                Scripted {
                    serial: st.serial.get(),
                    component,
                    template,
                    parent,
                    props,
                },
            );
        }
        made
    }

    /// The objects scripts made under objects of the old tree, made again
    /// under the ones that go on for their parents, at the same paths, so
    /// that they take what the old ones held and a script that holds an
    /// old one reaches the new. One whose parent is not of the tree stays
    /// as it is; one whose parent is gone goes with it.
    fn reload_remake_scripted(&self) {
        let st = &self.inner.reload;
        let mut all: Vec<(ObjectId, Scripted)> = st.scripted.borrow_mut().drain().collect();
        all.sort_by_key(|(_, s)| s.serial);
        let tree = st.tree.get();
        for (old, s) in all {
            let path = st
                .info
                .borrow()
                .get(&old)
                .filter(|i| i.tree != tree)
                .map(|i| i.path.clone());
            let counterpart = |o: ObjectId| {
                st.pending
                    .borrow()
                    .as_ref()
                    .and_then(|p| p.counterpart.get(&o).copied())
            };
            let (Some(path), Some(parent)) = (path, counterpart(s.parent)) else {
                if self.is_alive(old) {
                    st.scripted.borrow_mut().insert(old, s);
                }
                continue;
            };
            // A component declared in a file is the one that goes on for
            // it; one made from a file's address is made from the file as
            // it is now.
            let (component, template) = match counterpart(s.component) {
                Some(c) => match self.component_template(c) {
                    Ok(t) => (c, t),
                    Err(_) => continue,
                },
                None if s.template.outer.is_none() => {
                    (s.component, self.fresh_template(&s.template))
                }
                None => continue,
            };
            let props: Vec<(String, Value)> = s
                .props
                .iter()
                .filter_map(|(k, v)| self.reload_translate(v.clone()).map(|v| (k.clone(), v)))
                .collect();
            *st.forced.borrow_mut() = Some(path);
            let made =
                self.create_for_script(component, template, Some(parent), props, Initial::Created);
            st.forced.borrow_mut().take();
            if let Err(e) = made {
                self.warn(format!("{e}"));
            }
        }
    }

    /// `template` made from its file as the files are now: a reload
    /// replaced the file a `Component` was made from by address.
    pub(crate) fn fresh_template(&self, template: &Rc<Template>) -> Rc<Template> {
        if template.outer.is_some()
            || template.context.is_some()
            || !self.inner.reload.tracking.get()
        {
            return template.clone();
        }
        let Ok(now) = self.load_file(&template.file.key) else {
            return template.clone();
        };
        if Rc::ptr_eq(&now, &template.file) {
            return template.clone();
        }
        let def = template.file.ir.component(template.comp);
        let inline =
            matches!(def.kind, ljos_ir::ComponentKind::Inline).then_some(def.name.as_str());
        let Some(comp) = now.find_component(inline) else {
            return template.clone();
        };
        Rc::new(Template {
            file: now,
            comp,
            outer: None,
            context: None,
        })
    }

    fn reload_note_forwards(&self) {
        let st = &self.inner.reload;
        let pending = st.pending.borrow();
        let Some(p) = pending.as_ref() else { return };
        let mut forwards = st.forwards.borrow_mut();
        for (to, _) in forwards.values_mut() {
            if let Some(new) = p.counterpart.get(to) {
                *to = *new;
            }
        }
        forwards.retain(|_, (to, _)| self.is_alive(*to));
        let Some(js) = self.inner.js.borrow().clone() else {
            return;
        };
        // Names, not the type: a type held here would keep its script
        // prototype from being released.
        let mut by_type: HashMap<*const MetaObject, Rc<OldMembers>> = HashMap::default();
        for (old, new) in &p.counterpart {
            if !js.seen(*old) {
                continue;
            }
            let Some(meta) = self.meta_of(*old) else {
                continue;
            };
            let members = by_type
                .entry(&*meta as *const MetaObject)
                .or_insert_with(|| {
                    Rc::new(OldMembers {
                        props: meta.properties.iter().map(|i| i.name.clone()).collect(),
                        methods: meta.methods.iter().map(|m| m.name.clone()).collect(),
                    })
                });
            forwards.insert(*old, (*new, members.clone()));
        }
    }

    /// How many replaced objects a script may still hold, each reaching
    /// the object that went on for it.
    pub fn reload_forward_count(&self) -> usize {
        self.inner.reload.forwards.borrow().len()
    }

    /// Whether `obj` is there to make objects (a `Component`) or to give a
    /// model its rows (a `ListElement`), holding no state of its own.
    fn reload_template(&self, obj: ObjectId) -> bool {
        self.meta_of(obj).is_some_and(|m| {
            m.inherits(<crate::qtqml::Component as QmlType>::static_meta())
                || m.inherits(<crate::listmodel::ListElement as QmlType>::static_meta())
        })
    }

    /// Scripts let go of `objs`: a replaced one among them is reached no
    /// more. Every full reload replaces every object, so the forwards
    /// would otherwise grow by a tree each time.
    pub(crate) fn reload_let_go(&self, objs: &[u64]) {
        let st = &self.inner.reload;
        let Ok(mut forwards) = st.forwards.try_borrow_mut() else {
            return;
        };
        if forwards.is_empty() {
            return;
        }
        let js = self.inner.js.borrow().clone();
        for o in objs {
            let old = crate::js::u64_to_obj(*o);
            // A wrapper made again for it since still reaches it.
            if !js.as_ref().is_some_and(|js| js.seen(old)) {
                forwards.remove(&old);
            }
        }
    }

    /// The object that went on for `old`, which a reload replaced, with
    /// the names of `old`'s members; `None` for a live object or while reloads are not
    /// tracked.
    pub(crate) fn reload_forwarded(&self, old: ObjectId) -> Option<(ObjectId, Rc<OldMembers>)> {
        let st = &self.inner.reload;
        if !st.tracking.get() || self.is_alive(old) {
            return None;
        }
        let forwards = st.forwards.borrow();
        let (new, names) = forwards.get(&old)?;
        self.is_alive(*new).then(|| (*new, names.clone()))
    }

    /// Reads the old tree: what each object would carry, and what the
    /// patch says was edited.
    fn reload_snapshot(&self, patch: &ReloadPatch) -> Pending {
        let st = &self.inner.reload;
        let mut pending = Pending::default();
        for f in &patch.files {
            pending.changed_files.insert(f.file.file.clone());
        }
        for op in &patch.ops {
            match op {
                ReloadOp::PropertyChanged {
                    file,
                    object,
                    property,
                } => {
                    pending
                        .changed
                        .entry((file.clone(), object.clone()))
                        .or_default()
                        .push(property.clone());
                    if let Some(parent) = object.parent() {
                        pending.children_changed.insert((file.clone(), parent));
                    }
                }
                ReloadOp::ObjectAdded { file, object }
                | ReloadOp::ObjectRemoved { file, object } => {
                    if let Some(parent) = object.parent() {
                        pending.children_changed.insert((file.clone(), parent));
                    }
                }
                ReloadOp::ObjectMoved { file, from, to } => {
                    for key in [from, to] {
                        if let Some(parent) = key.parent() {
                            pending.children_changed.insert((file.clone(), parent));
                        }
                    }
                }
                _ => {}
            }
        }
        let objects: Vec<(ObjectId, Rc<str>, Rc<str>)> = st
            .info
            .borrow()
            .iter()
            .map(|(id, i)| (*id, i.path.clone(), i.print.clone()))
            .collect();
        let state_props = st.state_props.borrow().clone();
        let migrators = st.migrators.borrow().clone();
        for (obj, path, print) in objects {
            // One on its way out whose place another took (a row made
            // again) has nothing to carry.
            if st.by_path.borrow().get(&path).is_some_and(|o| *o != obj) {
                continue;
            }
            let Some(Taken { values, native }) = self.reload_take(obj, &state_props, &migrators)
            else {
                continue;
            };
            pending.snaps.insert(
                path,
                Snap {
                    old: obj,
                    print,
                    values,
                    native,
                    taker: None,
                },
            );
        }
        pending
    }

    /// What `obj` holds that a reload carries to the object that goes on
    /// for it: what scripts assigned, the registered state properties
    /// that left their default, and what its migrators keep.
    pub(super) fn reload_take(
        &self,
        obj: ObjectId,
        state_props: &[(String, String)],
        migrators: &[Rc<Migrator>],
    ) -> Option<Taken> {
        let st = &self.inner.reload;
        let meta = self.meta_of(obj)?;
        let mut values: Vec<Carried> = Vec::new();
        let assigned = st.assigned.borrow().get(&obj).cloned().unwrap_or_default();
        for prop in assigned {
            if let Some(c) = self.reload_carry(obj, &meta, prop, true) {
                values.push(c);
            }
        }
        for (ty, name) in state_props {
            if !(ty.is_empty() || meta.inherits_name(ty)) {
                continue;
            }
            let Some(prop) = meta.property_index(name) else {
                continue;
            };
            if values.iter().any(|c| c.name == name.as_str()) {
                continue;
            }
            if self
                .provenance(obj, prop)
                .is_none_or(|p| p == Provenance::Default)
            {
                continue;
            }
            if let Some(c) = self.reload_carry(obj, &meta, prop, false) {
                values.push(c);
            }
        }
        let mut native = Vec::new();
        for (i, m) in migrators.iter().enumerate() {
            if meta.inherits(m.meta)
                && let Some(state) = (m.snapshot)(self, obj)
            {
                native.push((i, state));
            }
        }
        Some(Taken { values, native })
    }

    fn reload_carry(
        &self,
        obj: ObjectId,
        meta: &MetaRef,
        prop: PropIndex,
        script: bool,
    ) -> Option<Carried> {
        let info = meta.property(prop)?;
        if info.is_alias() || info.flags.contains(PropertyFlags::GROUP) || self.is_bound(obj, prop)
        {
            return None;
        }
        // What an override holds (a state, a `Binding`) is not the
        // property's own value: carried, it would be what the property goes
        // back to when the new file's state lets go of it.
        let (at, slot) = self.redirect(obj, prop);
        let node = self
            .inner
            .store
            .borrow()
            .objects
            .get(at)
            .and_then(|h| h.node(slot));
        if node.is_some_and(|n| self.inner.graph.override_count(n) > 0) {
            return None;
        }
        // An object it holds is put right for the new tree when the value
        // is given back (`reload_adopt`).
        let value = self.untrack(|| self.read(obj, prop));
        Some(Carried {
            name: info.name.clone(),
            ty: info.ty.clone(),
            value,
            script,
        })
    }

    /// Without `clear_metas` the types made for objects of the files stay:
    /// the patch declares nothing anew (in_place.rs).
    fn reload_patch_loader(
        &self,
        patch: &ReloadPatch,
        clear_metas: bool,
    ) -> Result<SavedLoader, ReloadError> {
        let st = &self.inner.reload;
        let mut l = self.inner.loader.borrow_mut();
        let current = l.source.clone().ok_or(ReloadError::NoSource)?;
        let previous = st.overlay.borrow_mut().take();
        let ours = previous
            .as_ref()
            .is_some_and(|o| std::ptr::addr_eq(Rc::as_ptr(&o.installed), Rc::as_ptr(&current)));
        let (base, mut files) = match &previous {
            Some(o) if ours => (o.base.clone(), o.files.clone()),
            // The host gave the engine other files since.
            _ => (current.clone(), HashMap::default()),
        };
        for f in &patch.files {
            files.insert(f.file.file.clone(), Some(Arc::new(f.file.clone())));
        }
        for key in &patch.removed {
            files.insert(key.clone(), None);
        }
        let saved = SavedLoader {
            source: current,
            files: l.files.clone(),
            metas: l
                .files
                .values()
                .map(|f| (f.clone(), f.metas.borrow().clone()))
                .collect(),
            overlay: previous,
        };
        for f in &patch.files {
            l.files.remove(&f.file.file);
        }
        for key in &patch.removed {
            l.files.remove(key);
        }
        // The type made for an object of an unchanged file may derive
        // from one of a changed file.
        if clear_metas {
            for f in l.files.values() {
                f.metas.borrow_mut().clear();
                f.unlink();
            }
        }
        let installed: Rc<dyn ComponentSource> = Rc::new(PatchedSource {
            base: base.clone(),
            files: files.clone(),
        });
        l.source = Some(installed.clone());
        *st.overlay.borrow_mut() = Some(Overlay {
            installed,
            base,
            files,
        });
        drop(l);
        // A singleton is no root: it is remade when its file is another.
        self.singletons_sources_changed();
        Ok(saved)
    }

    fn reload_restore_loader(&self, saved: SavedLoader) {
        let mut l = self.inner.loader.borrow_mut();
        l.source = Some(saved.source);
        l.files = saved.files;
        for (file, metas) in saved.metas {
            *file.metas.borrow_mut() = metas;
            file.unlink();
        }
        *self.inner.reload.overlay.borrow_mut() = saved.overlay;
        drop(l);
        self.singletons_sources_changed();
    }

    /// `e` as a place and a message. The engine's errors about a file
    /// start with `url:line:column: ` or `url: `. Public for a host that
    /// makes a root itself and shows why it could not.
    pub fn reload_fault(&self, e: &EngineError) -> ReloadFault {
        let text = match e {
            // The variant holds the whole message; its Display wraps it
            // once more.
            EngineError::RequiredNotSet(s) => s.clone(),
            other => other.to_string(),
        };
        for (i, _) in text.match_indices(": ") {
            let head = &text[..i];
            let message = text[i + 2..].to_owned();
            let mut parts = head.rsplitn(3, ':');
            let (column, line, url) = (parts.next(), parts.next(), parts.next());
            if let (Some(c), Some(l), Some(url)) = (column, line, url)
                && let (Ok(column), Ok(line)) = (c.parse::<u32>(), l.parse::<u32>())
            {
                return ReloadFault {
                    file: format!("{}.qml", self.url_key(url)),
                    line,
                    column,
                    message,
                };
            }
            if head.ends_with(".qml") {
                return ReloadFault {
                    file: format!("{}.qml", self.url_key(head)),
                    line: 0,
                    column: 0,
                    message,
                };
            }
        }
        ReloadFault {
            file: String::new(),
            line: 0,
            column: 0,
            message: text,
        }
    }

    // ------------------------------------------------- hooks of creation

    /// Whether a QML handler that dies with `scope` is to stay silent
    /// now.
    pub(crate) fn reload_muted(&self, scope: ObjectId) -> bool {
        let st = &self.inner.reload;
        st.muting.get() && (st.muted_all.get() || st.muted.borrow().contains(&scope))
    }

    /// A script assigned `prop` of `obj`.
    pub(crate) fn reload_note_assigned(&self, obj: ObjectId, prop: PropIndex) {
        let st = &self.inner.reload;
        let (obj, prop) = self.redirect(obj, prop);
        if !st.info.borrow().contains_key(&obj) {
            return;
        }
        let mut assigned = st.assigned.borrow_mut();
        let props = assigned.entry(obj).or_default();
        if !props.contains(&prop) {
            props.push(prop);
        }
    }

    /// `root` was asked for by URL: a root a reload can make again.
    pub(crate) fn reload_note_origin(
        &self,
        root: ObjectId,
        url: &str,
        parent: Option<ObjectId>,
        props: &[(String, Value)],
        scope: Option<ObjectId>,
    ) {
        self.inner.reload.origins.borrow_mut().insert(
            root,
            Origin {
                url: url.to_owned(),
                parent,
                props: props.to_vec(),
                scope,
            },
        );
    }

    /// Keeps what a native type binds `prop` of `obj` to when not given
    /// it, while a reload may take a given value away.
    pub(crate) fn reload_default(&self, obj: ObjectId, prop: PropIndex, f: &DefaultBinding) {
        if self.inner.reload.tracking.get() {
            self.inner
                .reload
                .defaults
                .borrow_mut()
                .insert((obj, prop), f.clone());
        }
    }

    /// What a native type binds `prop` of `obj` to when not given it.
    pub(crate) fn reload_default_of(
        &self,
        obj: ObjectId,
        prop: PropIndex,
    ) -> Option<DefaultBinding> {
        self.inner
            .reload
            .defaults
            .borrow()
            .get(&(obj, prop))
            .cloned()
    }

    /// Objects are gone.
    pub(crate) fn reload_forget(&self, dead: &[ObjectId]) {
        let st = &self.inner.reload;
        st.numbered.borrow_mut().clear();
        let mut info = st.info.borrow_mut();
        if info.is_empty() {
            return;
        }
        let mut by_path = st.by_path.borrow_mut();
        let mut assigned = st.assigned.borrow_mut();
        let mut origins = st.origins.borrow_mut();
        let mut given = st.given.borrow_mut();
        let mut defaults = st.defaults.borrow_mut();
        for id in dead {
            if let Some(i) = info.remove(id)
                && by_path.get(&i.path) == Some(id)
            {
                by_path.remove(&i.path);
            }
            assigned.remove(id);
            origins.remove(id);
            given.remove(id);
        }
        if !defaults.is_empty() {
            let dead: HashSet<&ObjectId> = dead.iter().collect();
            defaults.retain(|(id, _), _| !dead.contains(id));
        }
        drop((info, by_path, assigned, origins, given, defaults));
        let mut scripted = st.scripted.borrow_mut();
        if !scripted.is_empty() {
            for id in dead {
                scripted.remove(id);
            }
        }
    }

    /// Destroys the root of a creation that failed. While a reload makes
    /// the new tree, nothing of it is to be heard.
    pub(crate) fn reload_discard(&self, root: ObjectId) {
        let st = &self.inner.reload;
        if st.muting.get() && !st.muted_all.get() {
            st.muted_all.set(true);
            self.destroy_now(root);
            st.muted_all.set(false);
        } else {
            self.destroy_now(root);
        }
    }

    /// Says that `root` is the delegate of `row`, before its paths are
    /// given.
    pub(crate) fn reload_note_row(&self, root: ObjectId, row: usize) {
        let st = &self.inner.reload;
        if st.tracking.get() {
            st.rows.borrow_mut().insert(root, row as i64);
        }
    }

    /// The row a delegate is made for.
    fn reload_row(&self, root: ObjectId, ctx: ContextId) -> Option<i64> {
        let v = self.untrack(|| {
            self.context_name(ctx, "index").or_else(|| {
                let p = self.meta_of(root)?.property_index("index")?;
                Some(self.read(root, p))
            })
        })?;
        match v {
            Value::Int(n) => Some(i64::from(n)),
            Value::Real(n) if n.is_finite() => Some(n as i64),
            _ => None,
        }
    }

    /// Gives the objects of the tree of `top` the paths they now have,
    /// after a reload in place added objects to it or removed some: the
    /// next reload finds them by those.
    pub(super) fn reload_repath(&self, top: &Rc<Instance>) {
        let st = &self.inner.reload;
        let Some(root) = top.objects.first() else {
            return;
        };
        let Some((prefix, tree)) = st.info.borrow().get(root).map(|i| (i.path.clone(), i.tree))
        else {
            return;
        };
        let mut walk = Walk::default();
        walk.instance(top, &prefix, true);
        let mut info = st.info.borrow_mut();
        let mut by_path = st.by_path.borrow_mut();
        for id in &walk.order {
            let Some(path) = walk.path.remove(id) else {
                continue;
            };
            let print = walk.print.remove(id).unwrap_or_default();
            if let Some(i) = info.get(id) {
                if *i.path == *path && *i.print == *print {
                    continue;
                }
                if by_path.get(&i.path) == Some(id) {
                    by_path.remove(&i.path);
                }
            }
            let path: Rc<str> = Rc::from(path);
            by_path.insert(path.clone(), *id);
            info.insert(
                *id,
                Info {
                    path,
                    print: Rc::from(print),
                    tree,
                },
            );
        }
    }

    /// Gives the objects of a creation their paths, once they exist and
    /// before anything of them runs, and says which of them go on for an
    /// object of the old tree.
    pub(super) fn reload_assign(&self, cr: &Creation, root: ObjectId) {
        let st = &self.inner.reload;
        if !st.tracking.get() {
            return;
        }
        let noted = st.rows.borrow_mut().remove(&root);
        let scripted = st.scripting.replace(false);
        let Some(top) = cr.top.clone() else {
            return;
        };
        self.reload_note_instances(&top);
        let forced = st.forced.borrow_mut().take();
        let above = self
            .owner(root)
            .and_then(|p| st.info.borrow().get(&p).map(|i| i.path.clone()));
        let mut root_path: String = match (forced, above) {
            (Some(p), _) => p.to_string(),
            (None, Some(above)) => {
                let def = top.file.ir.component(top.comp);
                let mut s = format!("{above}~{}|{}", top.file.key, def.id);
                if let Some(row) = noted.or_else(|| self.reload_row(root, top.context)) {
                    let _ = write!(s, "[{row}]");
                }
                s
            }
            (None, None) => {
                st.serial.set(st.serial.get() + 1);
                format!("@{}", st.serial.get())
            }
        };
        let tree = st.tree.get();
        // Two objects made from one component under one owner by a view
        // cannot be told apart: the later ones are not carried. A script's
        // are told apart by the order they were made in. One on its way
        // out (a row made again) gives its place up.
        let taken = |path: &str| {
            let by_path = st.by_path.borrow();
            let info = st.info.borrow();
            by_path.get(path).is_some_and(|other| {
                *other != root
                    && info.get(other).is_some_and(|i| i.tree == tree)
                    && self.is_alive(*other)
                    && !self.doomed(*other)
            })
        };
        if taken(&root_path) {
            if !scripted {
                return;
            }
            let base = std::mem::take(&mut root_path);
            let mut n = st
                .numbered
                .borrow()
                .get(base.as_str())
                .copied()
                .unwrap_or(2);
            root_path.reserve(base.len() + 4);
            loop {
                root_path.clear();
                root_path.push_str(&base);
                let _ = write!(root_path, "*{n}");
                if !taken(&root_path) {
                    break;
                }
                n += 1;
            }
            st.numbered.borrow_mut().insert(base, n + 1);
        }
        let mut walk = Walk::default();
        walk.instance(&top, &root_path, true);
        let owners: Vec<Option<ObjectId>> = walk.order.iter().map(|o| self.owner(*o)).collect();
        let mut pending = st.pending.borrow_mut();
        let mut info = st.info.borrow_mut();
        let mut by_path = st.by_path.borrow_mut();
        for (id, owner) in walk.order.iter().zip(owners) {
            let Some(path) = walk.path.remove(id) else {
                continue;
            };
            let path: Rc<str> = Rc::from(path);
            let print: Rc<str> = Rc::from(walk.print.remove(id).unwrap_or_default());
            if let Some(p) = pending.as_mut() {
                let below = owner.is_some_and(|o| p.remounted.contains(&o));
                let reset = walk.reset.contains(id);
                let free = |s: &Snap| s.taker.is_none_or(|t| self.doomed(t));
                match p.snaps.get_mut(&path) {
                    _ if below => {
                        p.remounted.insert(*id);
                    }
                    Some(s) if !reset && free(s) && s.print == print => {
                        if let Some(gone) = s.taker.replace(*id) {
                            p.kept.remove(&gone);
                        }
                        p.counterpart.insert(s.old, *id);
                        p.kept.insert(*id, path.clone());
                    }
                    Some(s) if free(s) => {
                        p.remounted.insert(*id);
                    }
                    None if reset => {
                        p.remounted.insert(*id);
                    }
                    // Often gone once it was used (a style's part made,
                    // a model's rows read): nothing in the old tree to go
                    // on for, and nothing lost.
                    _ if self.reload_template(*id) => {}
                    _ => p.fresh += 1,
                }
            }
            by_path.insert(path.clone(), *id);
            info.insert(*id, Info { path, print, tree });
        }
    }

    /// A carried value with its objects of the old tree swapped for the
    /// ones that go on for them (see [`translate`]); `None` when it is an
    /// object that is gone. An object made later than the one the value
    /// goes to (a delegate) is not known yet, and counts as gone. For a
    /// [`Migrate`] whose state holds objects; valid until
    /// [`Engine::finish_reload`].
    pub fn reload_translate(&self, v: Value) -> Option<Value> {
        let st = &self.inner.reload;
        let tree = st.tree.get();
        let pending = st.pending.borrow();
        let info = st.info.borrow();
        // An object of an earlier making that no root being made again
        // owns (a singleton's) outlives the reload as it is.
        let goes = |o: ObjectId| {
            let Some(p) = pending.as_ref() else {
                return false;
            };
            let mut cur = Some(o);
            while let Some(c) = cur {
                if p.old_roots.contains(&c) {
                    return true;
                }
                cur = self.owner(c);
            }
            false
        };
        let new_for = |o: ObjectId| {
            if let Some(new) = pending.as_ref().and_then(|p| p.counterpart.get(&o)) {
                return Some(Some(*new));
            }
            info.get(&o)
                .filter(|i| i.tree != tree && goes(o))
                .map(|_| None)
        };
        translate(v, &|o| self.is_alive(o), &new_for)
    }

    /// Gives the kept objects of a creation what their old objects held,
    /// before the creation completes. Returns the kept objects: they do
    /// not run `Component.onCompleted`.
    pub(super) fn reload_adopt(&self, cr: &Creation) -> HashSet<ObjectId> {
        let st = &self.inner.reload;
        if !st.tracking.get() || st.pending.borrow().is_none() {
            return HashSet::default();
        }
        type Work = (ObjectId, Vec<Carried>, Vec<(usize, Box<dyn Any>)>);
        let mut work: Vec<Work> = Vec::new();
        // (object, the edited properties as written)
        let mut edited: Vec<(ObjectId, Vec<String>)> = Vec::new();
        let mut inside_edited: HashSet<ObjectId> = HashSet::default();
        // (old, new) for each kept object
        let mut pairs: Vec<(ObjectId, ObjectId)> = Vec::new();
        {
            let mut pending = st.pending.borrow_mut();
            let Some(p) = pending.as_mut() else {
                return HashSet::default();
            };
            for o in &cr.all {
                let Some(path) = p.kept.get(o) else { continue };
                if let Some(s) = p.snaps.get_mut(path) {
                    // Cloned: an object made again for the same place
                    // takes them too.
                    work.push((*o, s.values.clone(), std::mem::take(&mut s.native)));
                    pairs.push((s.old, *o));
                }
            }
            if work.is_empty() {
                return HashSet::default();
            }
            for u in &cr.units {
                if !p.changed_files.contains(&u.inst.file.key) {
                    continue;
                }
                let key = (
                    u.inst.file.key.clone(),
                    u.inst.file.ir.object_key(ObjRef {
                        component: u.inst.comp,
                        object: ObjIdx(u.obj as u32),
                    }),
                );
                let id = u.inst.objects[u.obj];
                if p.children_changed.contains(&key) {
                    inside_edited.insert(id);
                }
                if let Some(props) = p.changed.get(&key) {
                    edited.push((id, props.clone()));
                }
            }
        }
        // The file's word for a property it declares differently now.
        let mut skip: HashSet<(ObjectId, PropIndex)> = HashSet::default();
        for (id, props) in edited {
            for written in props {
                let mut path: Vec<String> = written.split('.').map(str::to_owned).collect();
                let Some(name) = path.pop() else { continue };
                let known = path.is_empty()
                    || self
                        .meta_of(id)
                        .is_some_and(|m| m.property_index(&path[0]).is_some())
                    || self.property_index(id, &written).is_ok();
                if !known {
                    // An attached property: what attached objects hold is
                    // not carried at all.
                    continue;
                }
                if let Ok(t) = self.untrack(|| self.member_target(id, &path, &name)) {
                    skip.insert(self.redirect(t.obj, t.prop));
                }
            }
        }
        let migrators = st.migrators.borrow().clone();
        let mut kept: HashSet<ObjectId> =
            HashSet::with_capacity_and_hasher(work.len(), Default::default());
        let mut failed: Vec<ObjectId> = Vec::new();
        // A value coming back is not news: `onCheckedChanged` ran when
        // the user checked the box.
        let was_muting = (st.muting.get(), st.muted_all.get());
        st.muting.set(true);
        st.muted_all.set(true);
        self.batch(|| {
            for (obj, values, native) in work {
                if !self.is_alive(obj) {
                    continue;
                }
                let native = if inside_edited.contains(&obj) {
                    Vec::new()
                } else {
                    native
                };
                if self.reload_give(obj, Taken { values, native }, &skip, &migrators) {
                    kept.insert(obj);
                } else {
                    failed.push(obj);
                }
            }
        });
        st.muting.set(was_muting.0);
        st.muted_all.set(was_muting.1);
        // A kept instance's scripts hold what the application set them up
        // with, which its kept objects do not do again. The objects of an
        // instance share its context, so each pair of contexts is told once.
        let mut told: HashSet<(ContextId, ContextId)> = HashSet::default();
        for (old, new) in pairs {
            if !kept.contains(&new) {
                continue;
            }
            if let (Some(was), Some(is)) = (self.context_of(old), self.context_of(new))
                && was != is
                && told.insert((was, is))
            {
                self.scripts_go_on(was, is);
            }
        }
        if !failed.is_empty()
            && let Some(p) = st.pending.borrow_mut().as_mut()
        {
            for o in failed {
                p.kept.remove(&o);
                p.counterpart.retain(|_, new| *new != o);
                p.fresh += 1;
            }
        }
        kept
    }
}

impl Engine {
    /// Gives `obj` what [`Engine::reload_take`] took from the object it
    /// goes on for, but the properties of `skip`, which the file gives
    /// anew. `false`, with nothing written, when a migrator refused.
    pub(super) fn reload_give(
        &self,
        obj: ObjectId,
        taken: Taken,
        skip: &HashSet<(ObjectId, PropIndex)>,
        migrators: &[Rc<Migrator>],
    ) -> bool {
        let mut ok = true;
        for (i, state) in taken.native {
            let Some(m) = migrators.get(i) else { continue };
            if let Err(why) = (m.migrate)(self, obj, state) {
                self.warn(format!(
                    "{}reload: {} is made anew, its state could not be carried: {why}",
                    self.declared_at(obj).unwrap_or_default(),
                    m.meta.name
                ));
                ok = false;
            }
        }
        if !ok {
            return false;
        }
        for mut c in taken.values {
            if holds_object(&c.value) {
                let Some(v) = self.reload_translate(c.value) else {
                    continue;
                };
                c.value = v;
            }
            let Ok(p) = self.property_index(obj, &c.name) else {
                continue;
            };
            let (to, tp) = self.redirect(obj, p);
            let same = self.meta_of(to).is_some_and(|m| {
                m.property(tp).is_some_and(|i| {
                    i.ty == c.ty
                        && !i.is_readonly()
                        && !i.is_alias()
                        && !i.flags.contains(PropertyFlags::GROUP)
                })
            });
            if !same || skip.contains(&(to, tp)) {
                continue;
            }
            if !c.script && self.is_bound(to, tp) {
                continue;
            }
            if self
                .write_internal(to, tp, c.value, WriteKind::Imperative)
                .is_ok()
                && c.script
            {
                self.reload_note_assigned(to, tp);
            }
        }
        true
    }
}

/// The migrators as a boundary restart uses them (boundary.rs): the same
/// [`Migrate`] implementations, without a reload.
impl Engine {
    pub(crate) fn has_migrators(&self) -> bool {
        !self.inner.reload.migrators.borrow().is_empty()
    }

    /// What `obj`'s migrators keep, each with the migrator's index.
    pub(crate) fn migrate_snapshot(&self, obj: ObjectId) -> Vec<(usize, Box<dyn Any>)> {
        let migrators = self.inner.reload.migrators.borrow().clone();
        let Some(meta) = self.meta_of(obj) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (i, m) in migrators.iter().enumerate() {
            if meta.inherits(m.meta)
                && let Some(state) = (m.snapshot)(self, obj)
            {
                out.push((i, state));
            }
        }
        out
    }

    /// Gives `obj` a state [`Engine::migrate_snapshot`] took from an
    /// object of the same type.
    pub(crate) fn migrate_restore(
        &self,
        obj: ObjectId,
        migrator: usize,
        state: Box<dyn Any>,
    ) -> Result<(), String> {
        let m = self.inner.reload.migrators.borrow().get(migrator).cloned();
        let Some(m) = m else {
            return Err("no such migrator".to_owned());
        };
        if !self.meta_of(obj).is_some_and(|meta| meta.inherits(m.meta)) {
            return Err("the object is of another type".to_owned());
        }
        (m.migrate)(self, obj, state)
    }
}

/// How long each step of a reload took, told at debug level
/// (`LJOS_LOG=ljos_engine::incubator::reload=debug`): what to cut when a
/// reload is slow.
struct Phases {
    last: std::time::Instant,
    took: Vec<(&'static str, std::time::Duration)>,
}

impl Phases {
    fn start() -> Self {
        Self {
            last: std::time::Instant::now(),
            took: Vec::new(),
        }
    }

    fn mark(&mut self, step: &'static str) {
        let now = std::time::Instant::now();
        self.took.push((step, now - self.last));
        self.last = now;
    }

    fn done(self) {
        if tracing::enabled!(tracing::Level::DEBUG) {
            let steps: Vec<String> = self
                .took
                .iter()
                .map(|(step, d)| format!("{step} {:.1} ms", d.as_secs_f64() * 1000.0))
                .collect();
            tracing::debug!("reload steps: {}", steps.join(", "));
        }
    }
}

impl Engine {
    /// An object is on its way out: the places of a script's roots may come
    /// free (see `ReloadState::numbered`).
    pub(crate) fn reload_doomed(&self) {
        let st = &self.inner.reload;
        if st.tracking.get() {
            st.numbered.borrow_mut().clear();
        }
    }
}
