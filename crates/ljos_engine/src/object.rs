//! The native side of an object ([`QmlObject`]) and the engine's per-object
//! record ([`ObjectHeader`], ARCHITECTURE.md §6.1).

use std::any::{Any, TypeId};
use std::rc::Rc;

use ljos_reactive::{NodeId, Reader, ScopeId};
use smallvec::SmallVec;

use crate::atom::Atom;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{ContextId, MethodIndex, ObjectId, PropIndex};
use crate::meta::{AttachedTypeId, MetaObject, MetaRef};
use crate::value::Value;

/// The typed Rust fields of an object. Implemented by
/// `#[derive(QmlObject)]`.
///
/// A native type that extends another holds it as a field (`#[qml(base)]`)
/// and returns it from [`QmlObject::base`]; the engine walks that chain to
/// find the struct that declares a property.
pub trait QmlObject: Any {
    /// The description of the Rust type (not of a QML type built on it).
    fn meta(&self) -> &'static MetaObject;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// The base type's part of this object.
    fn base(&self) -> Option<&dyn QmlObject> {
        None
    }
    fn base_mut(&mut self) -> Option<&mut dyn QmlObject> {
        None
    }
}

/// Static facts about a native type, for generated code.
pub trait QmlType: QmlObject + Sized {
    /// Length of the flattened property table. A constant, so derived types
    /// can number their own properties at compile time.
    const PROPERTY_COUNT: u32;
    fn static_meta() -> &'static MetaObject;
}

/// The part of `root`'s native chain that is a `ty`.
pub fn find_native(root: &dyn QmlObject, ty: TypeId) -> Option<&dyn QmlObject> {
    let mut cur = Some(root);
    while let Some(o) = cur {
        if o.as_any().type_id() == ty {
            return Some(o);
        }
        cur = o.base();
    }
    None
}

/// Mutable [`find_native`].
pub fn find_native_mut(root: &mut dyn QmlObject, ty: TypeId) -> Option<&mut dyn QmlObject> {
    if root.as_any().type_id() == ty {
        return Some(root);
    }
    root.base_mut().and_then(|b| find_native_mut(b, ty))
}

/// The `T` in `root`'s native chain.
pub fn downcast_chain<T: QmlObject>(root: &dyn QmlObject) -> Option<&T> {
    find_native(root, TypeId::of::<T>())?
        .as_any()
        .downcast_ref()
}

/// Mutable [`downcast_chain`].
pub fn downcast_chain_mut<T: QmlObject>(root: &mut dyn QmlObject) -> Option<&mut T> {
    find_native_mut(root, TypeId::of::<T>())?
        .as_any_mut()
        .downcast_mut()
}

bitflags::bitflags! {
    /// Lifecycle and ownership state.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
    pub struct ObjectFlags: u16 {
        /// `on_complete` has run.
        const COMPLETED = 1 << 0;
        /// Owned by the JS garbage collector rather than a parent.
        const JS_OWNED = 1 << 1;
        /// `destroy()` was called; it goes at the next deferred-delete pass.
        const PENDING_DELETE = 1 << 2;
        /// Parked in a delegate pool.
        const POOLED = 1 << 3;
        /// Its scope is frozen (off-screen, §8.3).
        const FROZEN = 1 << 4;
        /// Being torn down right now.
        const DESTROYING = 1 << 5;
        /// `Component.onDestruction` has run for it and what it owns.
        const DESTRUCTION_TOLD = 1 << 6;
    }
}

/// Everything the engine keeps per object. Fields are crate-private; the
/// engine's API is the way in.
pub struct ObjectHeader {
    pub(crate) meta: MetaRef,
    pub(crate) native: Box<dyn QmlObject>,
    /// QML-declared properties, by slot.
    pub(crate) dynamic: Box<[Value]>,
    /// The QObject parent: destroying it destroys this.
    pub(crate) owner: Option<ObjectId>,
    pub(crate) children: Vec<ObjectId>,
    /// The Item parent; may differ from the owner.
    pub(crate) visual_parent: Option<ObjectId>,
    pub(crate) visual_children: Vec<ObjectId>,
    pub(crate) context: ContextId,
    /// Attached objects, created on first use.
    pub(crate) attached: SmallVec<[(AttachedTypeId, ObjectId); 1]>,
    /// What only some objects need (QML-declared types, aliases, scripts).
    pub(crate) ext: Option<Box<ObjectExt>>,
    pub(crate) flags: ObjectFlags,
    /// Graph scope owning this object's nodes, created with the first node.
    pub(crate) scope: Option<ScopeId>,
    /// Reactive node per property, allocated on first observation or bind.
    pub(crate) nodes: Option<Box<[Option<NodeId>]>>,
    /// Properties written while they had no node, for provenance.
    pub(crate) assigned: SmallVec<[u64; 2]>,
    /// Tracked reads of properties that have no node.
    pub(crate) field_reads: Option<Box<FieldReads>>,
}

/// Who read which property of an object while it had no node. Most
/// properties are read and never change (a delegate's colour, a text's
/// font): keeping the read here costs far less than a node made for it
/// and disposed with the object. The property gets its node, read by
/// exactly the runs that read it, when it next changes
/// ([`Engine::ensure_node`]).
pub(crate) struct FieldReads {
    /// A bit per property with an entry.
    props: Box<[u64]>,
    entries: Vec<(PropIndex, Reader)>,
    /// Length past which entries of finished runs are swept out.
    sweep_at: usize,
    /// The run that read last, and a bit per property it read: a run
    /// that reads a property again adds nothing.
    last: Option<Reader>,
    last_props: Box<[u64]>,
}

impl FieldReads {
    /// For an object with `len` properties.
    pub(crate) fn new(len: usize) -> Self {
        let words = len.div_ceil(64);
        FieldReads {
            props: vec![0; words].into_boxed_slice(),
            entries: Vec::new(),
            sweep_at: 0,
            last: None,
            last_props: vec![0; words].into_boxed_slice(),
        }
    }

    pub(crate) fn has(&self, prop: PropIndex) -> bool {
        has_bit(&self.props, prop)
    }

    /// Adds a read, unless the run already made it. `current` tells
    /// whether an entry still counts, for the sweep.
    pub(crate) fn add(&mut self, prop: PropIndex, r: Reader, current: impl Fn(Reader) -> bool) {
        if self.last == Some(r) {
            if has_bit(&self.last_props, prop) {
                return;
            }
        } else {
            self.last = Some(r);
            self.last_props.iter_mut().for_each(|w| *w = 0);
        }
        set_bit(&mut self.last_props, prop);
        if self.entries.len() >= self.sweep_at {
            self.entries.retain(|(_, e)| current(*e));
            self.props.iter_mut().for_each(|w| *w = 0);
            for (p, _) in &self.entries {
                set_bit(&mut self.props, *p);
            }
            self.sweep_at = (self.entries.len() * 2).max(32);
        }
        set_bit(&mut self.props, prop);
        self.entries.push((prop, r));
    }

    /// Takes the readers of `prop` out.
    pub(crate) fn take(&mut self, prop: PropIndex) -> SmallVec<[Reader; 4]> {
        if !self.has(prop) {
            return SmallVec::new();
        }
        let (word, bit) = (prop.0 as usize / 64, prop.0 % 64);
        self.props[word] &= !(1 << bit);
        let mut out = SmallVec::new();
        self.entries.retain(|(p, r)| {
            let mine = *p == prop;
            if mine {
                out.push(*r);
            }
            !mine
        });
        out
    }

    /// Every reader of every property.
    pub(crate) fn all(&self) -> impl Iterator<Item = Reader> + '_ {
        self.entries.iter().map(|(_, r)| *r)
    }

    /// What the runs of `reader` read, each with the run that read it.
    pub(crate) fn readers_of(
        &self,
        reader: NodeId,
    ) -> impl Iterator<Item = (PropIndex, Reader)> + '_ {
        self.entries
            .iter()
            .filter(move |(_, r)| r.node == reader)
            .copied()
    }

    /// The readers of `prop`, left in place.
    pub(crate) fn readers(&self, prop: PropIndex) -> impl Iterator<Item = Reader> + '_ {
        self.entries
            .iter()
            .filter(move |(p, _)| *p == prop)
            .map(|(_, r)| *r)
    }
}

fn has_bit(words: &[u64], prop: PropIndex) -> bool {
    let (word, bit) = (prop.0 as usize / 64, prop.0 % 64);
    words.get(word).is_some_and(|w| w & (1 << bit) != 0)
}

fn set_bit(words: &mut [u64], prop: PropIndex) {
    let (word, bit) = (prop.0 as usize / 64, prop.0 % 64);
    if let Some(w) = words.get_mut(word) {
        *w |= 1 << bit;
    }
}

/// A native whose type builds on `QtObject` without a field for it (the
/// derive's implied base), with that part stored beside it. It is the
/// native to everything that looks at it: the chain walks to its
/// `QtObject` part as to a base field.
struct ImpliedBase {
    native: Box<dyn QmlObject>,
    base: crate::QtObject,
}

impl QmlObject for ImpliedBase {
    fn meta(&self) -> &'static MetaObject {
        self.native.meta()
    }
    fn as_any(&self) -> &dyn Any {
        self.native.as_any()
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self.native.as_any_mut()
    }
    fn base(&self) -> Option<&dyn QmlObject> {
        Some(&self.base)
    }
    fn base_mut(&mut self) -> Option<&mut dyn QmlObject> {
        Some(&mut self.base)
    }
}

/// `native`, with its `QtObject` part when its type has one only by the
/// derive's implied base.
fn with_implied_base(native: Box<dyn QmlObject>) -> Box<dyn QmlObject> {
    let qt_object = TypeId::of::<crate::QtObject>();
    let implied = native.base().is_none()
        && native.as_any().type_id() != qt_object
        && native
            .meta()
            .base
            .as_ref()
            .is_some_and(|b| b.native == Some(qt_object));
    if implied {
        Box::new(ImpliedBase {
            native,
            base: crate::QtObject::default(),
        })
    } else {
        native
    }
}

impl ObjectHeader {
    pub(crate) fn new(meta: MetaRef, native: Box<dyn QmlObject>, context: ContextId) -> Self {
        let native = with_implied_base(native);
        // Declared properties come after the native ones they build on:
        // from the end, the walk stops once every slot has its default,
        // and a type with none declared walks nothing.
        let slots = meta.dynamic_slots as usize;
        let mut dynamic = vec![Value::Undefined; slots].into_boxed_slice();
        let mut filled = vec![false; slots];
        let mut left = slots;
        for p in meta.properties.iter().rev() {
            if left == 0 {
                break;
            }
            if let crate::meta::PropertyAccess::Dynamic { slot } = p.access
                && !std::mem::replace(&mut filled[slot as usize], true)
            {
                dynamic[slot as usize] = p.ty.default_value();
                left -= 1;
            }
        }
        ObjectHeader {
            meta,
            native,
            dynamic,
            owner: None,
            children: Vec::new(),
            visual_parent: None,
            visual_children: Vec::new(),
            context,
            attached: SmallVec::new(),
            ext: None,
            flags: ObjectFlags::empty(),
            scope: None,
            nodes: None,
            assigned: SmallVec::new(),
            field_reads: None,
        }
    }

    pub(crate) fn node(&self, prop: PropIndex) -> Option<NodeId> {
        self.nodes.as_ref()?.get(prop.0 as usize).copied().flatten()
    }

    /// Whether a reader may follow `prop` without its node: a change has
    /// to make the node first ([`Engine::ensure_node`]) to reach it.
    pub(crate) fn read_without_node(&self, prop: PropIndex) -> bool {
        self.field_reads.as_ref().is_some_and(|r| r.has(prop))
    }

    pub(crate) fn set_node(&mut self, prop: PropIndex, n: NodeId) {
        let len = self.meta.properties.len();
        let nodes = self
            .nodes
            .get_or_insert_with(|| vec![None; len].into_boxed_slice());
        nodes[prop.0 as usize] = Some(n);
    }

    pub(crate) fn mark_assigned(&mut self, prop: PropIndex) {
        let (word, bit) = (prop.0 as usize / 64, prop.0 % 64);
        if self.assigned.len() <= word {
            self.assigned.resize(word + 1, 0);
        }
        self.assigned[word] |= 1 << bit;
    }

    pub(crate) fn clear_assigned(&mut self, prop: PropIndex) {
        let (word, bit) = (prop.0 as usize / 64, prop.0 % 64);
        if let Some(w) = self.assigned.get_mut(word) {
            *w &= !(1 << bit);
        }
    }

    pub(crate) fn is_assigned(&self, prop: PropIndex) -> bool {
        let (word, bit) = (prop.0 as usize / 64, prop.0 % 64);
        self.assigned.get(word).is_some_and(|w| w & (1 << bit) != 0)
    }

    pub(crate) fn ext_mut(&mut self) -> &mut ObjectExt {
        self.ext.get_or_insert_with(Box::default)
    }

    /// The target of an alias property.
    pub(crate) fn alias(&self, prop: PropIndex) -> Option<AliasTarget> {
        self.ext
            .as_ref()?
            .aliases
            .iter()
            .find(|(p, _)| *p == prop)
            .map(|(_, t)| t.clone())
    }
}

/// A method declared in QML: the engine calls it with `this` and the
/// arguments; the JS host runs it.
pub type ScriptMethod = Rc<dyn Fn(&Engine, ObjectId, &[Value]) -> Result<Value, EngineError>>;

/// Where an alias points: an object, one of its properties, or one member
/// of a property whose value has members (`label.font.family`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AliasTarget {
    pub(crate) object: ObjectId,
    pub(crate) prop: Option<PropIndex>,
    pub(crate) member: Option<Atom>,
}

/// A source position, for Qt-style messages.
#[derive(Clone, Debug)]
pub(crate) struct Location {
    pub(crate) url: Atom,
    pub(crate) line: u32,
    pub(crate) column: u32,
}

/// Where the binding on `prop` is written: the start of the property's
/// name, in the file that binds it (which is not always the file the
/// object is declared in).
#[derive(Clone, Debug)]
pub(crate) struct BindingLoc {
    pub(crate) prop: PropIndex,
    pub(crate) at: Location,
    /// What the binding is on when that is a part of the property:
    /// `font.pixelSize` for a binding stored on `font`.
    pub(crate) member: Option<Atom>,
}

/// Per-object state most native objects never need, boxed so the header
/// stays small.
#[derive(Default)]
pub(crate) struct ObjectExt {
    pub(crate) aliases: Vec<(PropIndex, AliasTarget)>,
    pub(crate) methods: Vec<(MethodIndex, ScriptMethod)>,
    pub(crate) location: Option<Location>,
    /// Where the binding of each property was written, for binding-loop
    /// messages.
    pub(crate) binding_locs: Vec<BindingLoc>,
    /// Contexts made for this object's component instance; destroyed with
    /// it.
    pub(crate) contexts: Vec<ContextId>,
    /// Read by bindings holding a reference to this object, so destroying
    /// it (which disposes the node with its scope) re-runs them.
    pub(crate) alive: Option<NodeId>,
    /// The visual parent as a reactive source.
    pub(crate) parent_node: Option<NodeId>,
    /// Data kept alive as long as the object (the component instance it
    /// roots, script state).
    pub(crate) keep: Vec<Rc<dyn Any>>,
    /// Sees every write from QML or JS first (a model row sends it to its
    /// model); true: the write is done.
    pub(crate) write_hook: Option<WriteHook>,
    /// The model this object is to a view (`model: obj`), for types that
    /// are one.
    pub(crate) model: Option<Rc<dyn crate::model::ItemModel>>,
    /// Read by views that name this object as their model, so one that
    /// resolved it before it became a model (completion runs
    /// last-made-first) resolves it again.
    pub(crate) model_node: Option<NodeId>,
    /// Values made when the property is next read
    /// ([`Engine::make_on_read`]).
    pub(crate) made_on_read: Vec<(PropIndex, crate::made_on_read::Maker)>,
}

/// See [`ObjectExt::write_hook`].
pub(crate) type WriteHook = Rc<dyn Fn(&Engine, PropIndex, &Value) -> bool>;
