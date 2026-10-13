//! The native side of an object ([`QmlObject`]) and the engine's per-object
//! record ([`ObjectHeader`], ARCHITECTURE.md §6.1).

use std::any::{Any, TypeId};
use std::rc::Rc;

use ljos_reactive::{NodeId, ScopeId};
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
    pub(crate) assigned: Vec<u64>,
}

impl ObjectHeader {
    pub(crate) fn new(meta: MetaRef, native: Box<dyn QmlObject>, context: ContextId) -> Self {
        let dynamic = meta
            .properties
            .iter()
            .filter_map(|p| match p.access {
                crate::meta::PropertyAccess::Dynamic { slot } => Some((slot, p.ty.default_value())),
                _ => None,
            })
            .fold(
                vec![Value::Undefined; meta.dynamic_slots as usize],
                |mut v, (slot, d)| {
                    v[slot as usize] = d;
                    v
                },
            )
            .into_boxed_slice();
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
            assigned: Vec::new(),
        }
    }

    pub(crate) fn node(&self, prop: PropIndex) -> Option<NodeId> {
        self.nodes.as_ref()?.get(prop.0 as usize).copied().flatten()
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
            .map(|(_, t)| *t)
    }
}

/// A method declared in QML: the engine calls it with `this` and the
/// arguments; the JS host runs it.
pub type ScriptMethod = Rc<dyn Fn(&Engine, ObjectId, &[Value]) -> Result<Value, EngineError>>;

/// Where an alias points: an object, or one of its properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AliasTarget {
    pub(crate) object: ObjectId,
    pub(crate) prop: Option<PropIndex>,
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
    /// Values made when the property is next read
    /// ([`Engine::make_on_read`]).
    pub(crate) made_on_read: Vec<(PropIndex, crate::made_on_read::Maker)>,
}

/// See [`ObjectExt::write_hook`].
pub(crate) type WriteHook = Rc<dyn Fn(&Engine, PropIndex, &Value) -> bool>;
