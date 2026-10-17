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

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::rc::Rc;
use std::sync::Arc;

use ljos_ir::{CompiledFile, KeySeg, ObjIdx, ObjRef, ObjectKey, ReloadOp, ReloadPatch};
use ljos_js::Bytecode;

use super::{Creation, Instance};
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

struct Migrator {
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

/// How a root was made, to make it again.
#[derive(Clone)]
struct Origin {
    url: String,
    parent: Option<ObjectId>,
    props: Vec<(String, Value)>,
}

#[derive(Clone)]
struct Carried {
    name: Atom,
    ty: TypeTag,
    value: Value,
    /// Assigned by a script, which outranks a binding of the new file.
    script: bool,
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

type MetaCache = HashMap<(u32, u32), MetaRef>;

/// What [`Engine::reload`] puts back when the new files do not work.
struct SavedLoader {
    source: Rc<dyn ComponentSource>,
    files: HashMap<String, Rc<LoadedFile>>,
    metas: Vec<(Rc<LoadedFile>, MetaCache)>,
    overlay: Option<Overlay>,
}

#[derive(Default)]
pub(crate) struct ReloadState {
    pub(crate) tracking: Cell<bool>,
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
    info: RefCell<HashMap<ObjectId, Info>>,
    by_path: RefCell<HashMap<Rc<str>, ObjectId>>,
    /// What scripts assigned, by the object that holds the value.
    assigned: RefCell<HashMap<ObjectId, Vec<PropIndex>>>,
    origins: RefCell<HashMap<ObjectId, Origin>>,
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
}

fn seg_name(seg: &KeySeg) -> String {
    match seg {
        KeySeg::Id(id) => format!("#{id}"),
        KeySeg::Named(name) => format!("'{name}'"),
        KeySeg::ModelRow(row) => format!("[{row}]"),
        KeySeg::Typed { ty, nth } => format!("{ty}[{nth}]"),
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
                (KeySeg::Id(name), _) => format!("{prefix}#{name}"),
                (seg, Some(parent)) => format!("{}/{}", local[parent.index()], seg_name(seg)),
            };
            if i > 0 || owns_root {
                // Two siblings of one objectName: numbered, as the
                // compiler's diff numbers them.
                let n = self.seen.entry(p.clone()).or_insert(0);
                *n += 1;
                if *n > 1 {
                    let _ = write!(p, "~{n}");
                }
                self.order.push(id);
                self.path.insert(id, p.clone());
            }
            let _ = write!(
                self.print.entry(id).or_default(),
                "{}|{}|{:?};",
                inst.file.key,
                def.id,
                od.ty
            );
            if reset {
                self.reset.insert(id);
            }
            if let Some(Some(inner)) = inst.inner.get(i) {
                let below = format!(
                    "{p}>{}|{}",
                    inner.file.key,
                    inner.file.ir.component(inner.comp).id
                );
                self.instance(inner, &below, false);
            }
            local.push(p);
        }
    }
}

/// Whether a value means the same in another tree of objects.
fn portable(v: &Value) -> bool {
    match v {
        Value::Object(_) => false,
        Value::List(items) => items.iter().all(portable),
        _ => true,
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
        if on && st.migrators.borrow().is_empty() {
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
        let pending = self.reload_snapshot(patch);
        let old: HashSet<ObjectId> = st.info.borrow().keys().copied().collect();
        let saved = self.reload_patch_loader(patch)?;
        st.epoch.set(st.epoch.get() + 1);
        st.tree.set(st.tree.get() + 1);
        *st.pending.borrow_mut() = Some(pending);
        // The old tree hears nothing of the new one being made beside it.
        *st.muted.borrow_mut() = old;
        st.muting.set(true);

        let mut made: Vec<(ObjectId, ObjectId)> = Vec::with_capacity(plan.len());
        let mut failure = None;
        for (old_root, origin, path) in &plan {
            *st.forced.borrow_mut() = Some(path.clone());
            match self.create_from_url(&origin.url, origin.parent, origin.props.clone()) {
                Ok(new) => made.push((*old_root, new)),
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            }
        }
        st.forced.borrow_mut().take();

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

        for (old_root, new) in &made {
            swap(self, *old_root, *new);
        }
        // An object that goes on is not told that it is destroyed; one
        // that is gone from the file is.
        let going_on: HashSet<ObjectId> = st
            .pending
            .borrow()
            .as_ref()
            .map(|p| p.counterpart.keys().copied().collect())
            .unwrap_or_default();
        *st.muted.borrow_mut() = going_on;
        for (old_root, _) in &made {
            self.destroy_now(*old_root);
        }
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
            let Some(meta) = self.meta_of(obj) else {
                continue;
            };
            let mut values: Vec<Carried> = Vec::new();
            let assigned = st.assigned.borrow().get(&obj).cloned().unwrap_or_default();
            for prop in assigned {
                if let Some(c) = self.reload_carry(obj, &meta, prop, true) {
                    values.push(c);
                }
            }
            for (ty, name) in &state_props {
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
        let value = self.untrack(|| self.read(obj, prop));
        portable(&value).then(|| Carried {
            name: info.name.clone(),
            ty: info.ty.clone(),
            value,
            script,
        })
    }

    fn reload_patch_loader(&self, patch: &ReloadPatch) -> Result<SavedLoader, ReloadError> {
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
            _ => (current.clone(), HashMap::new()),
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
        for f in l.files.values() {
            f.metas.borrow_mut().clear();
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
    ) {
        self.inner.reload.origins.borrow_mut().insert(
            root,
            Origin {
                url: url.to_owned(),
                parent,
                props: props.to_vec(),
            },
        );
    }

    /// Objects are gone.
    pub(crate) fn reload_forget(&self, dead: &[ObjectId]) {
        let st = &self.inner.reload;
        let mut info = st.info.borrow_mut();
        if info.is_empty() {
            return;
        }
        let mut by_path = st.by_path.borrow_mut();
        let mut assigned = st.assigned.borrow_mut();
        let mut origins = st.origins.borrow_mut();
        for id in dead {
            if let Some(i) = info.remove(id)
                && by_path.get(&i.path) == Some(id)
            {
                by_path.remove(&i.path);
            }
            assigned.remove(id);
            origins.remove(id);
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

    /// Gives the objects of a creation their paths, once they exist and
    /// before anything of them runs, and says which of them go on for an
    /// object of the old tree.
    pub(super) fn reload_assign(&self, cr: &Creation, root: ObjectId) {
        let st = &self.inner.reload;
        if !st.tracking.get() {
            return;
        }
        let noted = st.rows.borrow_mut().remove(&root);
        let Some(top) = cr.top.clone() else {
            return;
        };
        let forced = st.forced.borrow_mut().take();
        let above = self
            .owner(root)
            .and_then(|p| st.info.borrow().get(&p).map(|i| i.path.clone()));
        let root_path: String = match (forced, above) {
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
        // Two objects made from one component under one owner (two
        // `createObject` calls) cannot be told apart: the later ones are
        // not carried. One on its way out (a row made again) gives its
        // place up.
        let taken = {
            let by_path = st.by_path.borrow();
            let info = st.info.borrow();
            by_path.get(root_path.as_str()).is_some_and(|other| {
                *other != root
                    && info.get(other).is_some_and(|i| i.tree == tree)
                    && self.is_alive(*other)
                    && !self.doomed(*other)
            })
        };
        if taken {
            return;
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
                    _ => p.fresh += 1,
                }
            }
            by_path.insert(path.clone(), *id);
            info.insert(*id, Info { path, print, tree });
        }
    }

    /// Gives the kept objects of a creation what their old objects held,
    /// before the creation completes. Returns the kept objects: they do
    /// not run `Component.onCompleted`.
    pub(super) fn reload_adopt(&self, cr: &Creation) -> HashSet<ObjectId> {
        let st = &self.inner.reload;
        if !st.tracking.get() || st.pending.borrow().is_none() {
            return HashSet::new();
        }
        type Work = (ObjectId, Vec<Carried>, Vec<(usize, Box<dyn Any>)>);
        let mut work: Vec<Work> = Vec::new();
        // (object, the edited properties as written)
        let mut edited: Vec<(ObjectId, Vec<String>)> = Vec::new();
        let mut inside_edited: HashSet<ObjectId> = HashSet::new();
        {
            let mut pending = st.pending.borrow_mut();
            let Some(p) = pending.as_mut() else {
                return HashSet::new();
            };
            for o in &cr.all {
                let Some(path) = p.kept.get(o) else { continue };
                if let Some(s) = p.snaps.get_mut(path) {
                    // Cloned: an object made again for the same place
                    // takes them too.
                    work.push((*o, s.values.clone(), std::mem::take(&mut s.native)));
                }
            }
            if work.is_empty() {
                return HashSet::new();
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
        let mut skip: HashSet<(ObjectId, PropIndex)> = HashSet::new();
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
        let mut kept: HashSet<ObjectId> = HashSet::with_capacity(work.len());
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
                let mut ok = true;
                if !inside_edited.contains(&obj) {
                    for (i, state) in native {
                        let Some(m) = migrators.get(i) else { continue };
                        if let Err(why) = (m.migrate)(self, obj, state) {
                            self.warn(format!(
                                "{}reload: {} is made anew, its state could not be carried: {why}",
                                self.location_of(obj).unwrap_or_default(),
                                m.meta.name
                            ));
                            ok = false;
                        }
                    }
                }
                if !ok {
                    failed.push(obj);
                    continue;
                }
                kept.insert(obj);
                for c in values {
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
            }
        });
        st.muting.set(was_muting.0);
        st.muted_all.set(was_muting.1);
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
