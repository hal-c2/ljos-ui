//! Type descriptions: [`MetaObject`] and what it is built from.
//!
//! A `MetaObject` describes one QML type: its properties, signals and
//! methods, its base, default property and attached type. Native types get
//! a `'static` one from `#[derive(QmlObject)]`; types declared in QML get
//! one behind an `Arc`, because hot reload replaces them (§5.5). Both are
//! held as a [`MetaRef`].
//!
//! # Flattened tables
//!
//! Each table holds the inherited entries first, then the type's own. A
//! [`PropIndex`] therefore means the same property in a type and in every
//! type derived from it, which is what lets a typed handle such as
//! `Rectangle::RADIUS` be a compile-time constant and lets the compiler
//! resolve `final` properties to an index ahead of time. Grouped properties
//! are flattened too: `border.width` is an entry of `Rectangle` itself.
//! Name lookup goes through a per-meta map, built the first time a name is
//! looked up.

use std::any::TypeId;
use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::{Arc, OnceLock};

use crate::atom::Atom;
use crate::cx::Cx;
use crate::error::EngineError;
use crate::ids::{MethodIndex, PropIndex, SignalIndex};
use crate::object::QmlObject;
use crate::types::{ConvertError, TypeTag};
use crate::value::Value;

/// A shared reference to a type description: `'static` for native types,
/// reference-counted for types declared in QML.
#[derive(Clone)]
pub enum MetaRef {
    Static(&'static MetaObject),
    Shared(Arc<MetaObject>),
}

impl MetaRef {
    /// Whether both refer to the same description.
    pub fn ptr_eq(a: &MetaRef, b: &MetaRef) -> bool {
        std::ptr::eq::<MetaObject>(&**a, &**b)
    }

    /// The identity used as an attached-type key.
    pub fn type_id(&self) -> AttachedTypeId {
        AttachedTypeId(std::ptr::from_ref::<MetaObject>(self) as usize)
    }
}

impl Deref for MetaRef {
    type Target = MetaObject;
    fn deref(&self) -> &MetaObject {
        match self {
            MetaRef::Static(m) => m,
            MetaRef::Shared(m) => m,
        }
    }
}

impl From<&'static MetaObject> for MetaRef {
    fn from(m: &'static MetaObject) -> Self {
        MetaRef::Static(m)
    }
}

impl fmt::Debug for MetaRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MetaRef({})", self.name)
    }
}

/// Identifies an attaching type (`Keys`, `Layout`) by its description.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AttachedTypeId(pub usize);

bitflags::bitflags! {
    /// Per-property flags.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
    pub struct PropertyFlags: u16 {
        /// Not writable from QML or JS. Native code may still write it.
        const READONLY = 1 << 0;
        /// Cannot be shadowed; the compiler may resolve it statically.
        /// Every property declared in Rust is final.
        const FINAL = 1 << 1;
        /// Must be set by whoever creates the object.
        const REQUIRED = 1 << 2;
        /// The type's default property.
        const DEFAULT = 1 << 3;
        /// Never changes after creation.
        const CONSTANT = 1 << 4;
        /// The root of a grouped property (`border` of `border.width`).
        const GROUP = 1 << 5;
        /// A QML `list<T>` property: read as `Value::List`, filled through
        /// [`Engine::list_append`](crate::Engine::list_append).
        const LIST = 1 << 6;
        /// A `property alias`: reads, writes and bindings go to the target
        /// the object creator sets with
        /// [`Engine::set_alias`](crate::Engine::set_alias).
        const ALIAS = 1 << 7;
    }
}

/// Reads a native field. The argument is the struct that declares it.
pub type Getter = fn(&dyn QmlObject) -> Value;
/// Writes a native field. The argument is the struct that declares it.
pub type Setter = fn(&mut dyn QmlObject, &Value) -> Result<(), ConvertError>;
/// Gets the group struct (`Border`) from the struct that holds it.
pub type GroupGet = fn(&dyn QmlObject) -> &dyn QmlObject;
/// Mutable [`GroupGet`].
pub type GroupGetMut = fn(&mut dyn QmlObject) -> &mut dyn QmlObject;
/// A native method: arguments in, result out. JS calls it through the same
/// pointer.
pub type MethodThunk = fn(&Cx<'_>, &[Value]) -> Result<Value, EngineError>;
/// A native `on_complete` hook.
pub type CompleteThunk = fn(&Cx<'_>) -> Result<(), EngineError>;
/// Intercepts appends to a list property (`Item.data` sorts children into
/// `children` and `resources`). `cx.this` is the list's object.
pub type ListAppend = fn(&Cx<'_>, crate::ids::ObjectId) -> Result<(), EngineError>;

/// Where a property's value is stored.
#[derive(Clone, Copy)]
pub enum PropertyAccess {
    /// A field of the native struct `owner`, which is the object's native or
    /// one of its bases.
    Field {
        owner: TypeId,
        get: Getter,
        set: Setter,
    },
    /// A leaf of a grouped property: `leaf` of `meta` (the group's type),
    /// reached through a field of `owner`.
    Group {
        owner: TypeId,
        get: GroupGet,
        get_mut: GroupGetMut,
        meta: &'static MetaObject,
        leaf: PropIndex,
    },
    /// The group itself; it holds no value of its own.
    GroupRoot,
    /// A property declared in QML, stored in the object's dynamic slots.
    Dynamic { slot: u32 },
}

impl fmt::Debug for PropertyAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PropertyAccess::Field { .. } => f.write_str("Field"),
            PropertyAccess::Group { leaf, .. } => write!(f, "Group({})", leaf.0),
            PropertyAccess::GroupRoot => f.write_str("GroupRoot"),
            PropertyAccess::Dynamic { slot } => write!(f, "Dynamic({slot})"),
        }
    }
}

/// One property.
#[derive(Clone, Debug)]
pub struct PropertyInfo {
    pub name: Atom,
    pub ty: TypeTag,
    pub flags: PropertyFlags,
    /// The `nameChanged` signal, if the property has one.
    pub notify: Option<SignalIndex>,
    pub access: PropertyAccess,
    /// For a list property, the hook appends go through.
    pub list_append: Option<ListAppend>,
}

impl PropertyInfo {
    pub fn is_readonly(&self) -> bool {
        self.flags.contains(PropertyFlags::READONLY)
    }

    pub fn is_list(&self) -> bool {
        self.flags.contains(PropertyFlags::LIST)
    }

    pub fn is_alias(&self) -> bool {
        self.flags.contains(PropertyFlags::ALIAS)
    }
}

/// One enum of a type: `Component.Status` with `Null`, `Ready`, ...
#[derive(Clone, Debug)]
pub struct EnumInfo {
    pub name: Atom,
    pub keys: Vec<(Atom, i64)>,
}

/// One signal.
#[derive(Clone, Debug)]
pub struct SignalInfo {
    pub name: Atom,
    pub params: Vec<(Atom, TypeTag)>,
    /// For a synthesised change signal, the property it reports.
    pub notify_of: Option<PropIndex>,
}

/// One method.
#[derive(Clone)]
pub struct MethodInfo {
    pub name: Atom,
    pub params: Vec<(Atom, TypeTag)>,
    pub ret: TypeTag,
    /// `None` for methods declared in QML, which the JS host runs.
    pub thunk: Option<MethodThunk>,
}

impl fmt::Debug for MethodInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MethodInfo")
            .field("name", &self.name)
            .field("params", &self.params)
            .field("ret", &self.ret)
            .field("native", &self.thunk.is_some())
            .finish()
    }
}

#[derive(Default)]
struct Lookup {
    props: HashMap<Atom, PropIndex>,
    signals: HashMap<Atom, SignalIndex>,
    methods: HashMap<Atom, MethodIndex>,
}

/// The description of one QML type. See the module docs.
pub struct MetaObject {
    pub name: Atom,
    /// The module that exports it; `None` for types only reachable as a
    /// base, a group or an attached type.
    pub module: Option<Atom>,
    pub base: Option<MetaRef>,
    /// The Rust struct behind the type, for native types.
    pub native: Option<TypeId>,
    /// All properties, inherited first.
    pub properties: Vec<PropertyInfo>,
    pub signals: Vec<SignalInfo>,
    pub methods: Vec<MethodInfo>,
    /// Where this type's own entries start in each table.
    pub property_offset: u32,
    pub signal_offset: u32,
    pub method_offset: u32,
    /// Number of dynamic slots, inherited included.
    pub dynamic_slots: u32,
    pub default_property: Option<Atom>,
    /// The type `Name.` attaches to other objects (`Keys`, `Layout`).
    pub attached: Option<fn() -> MetaRef>,
    /// Makes the native part of a new object. Inherited by QML types.
    pub create: Option<fn() -> Box<dyn QmlObject>>,
    /// This type's own completion hook; the engine runs the chain base
    /// first.
    pub on_complete: Option<CompleteThunk>,
    /// Enums, inherited first.
    pub enums: Vec<EnumInfo>,
    lookup: OnceLock<Lookup>,
}

impl fmt::Debug for MetaObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetaObject")
            .field("name", &self.name)
            .field("module", &self.module)
            .field("base", &self.base.as_ref().map(|b| b.name.clone()))
            .finish_non_exhaustive()
    }
}

impl MetaObject {
    fn lookup(&self) -> &Lookup {
        self.lookup.get_or_init(|| {
            // Later entries win, so a derived type's property shadows a
            // base's of the same name.
            let mut l = Lookup::default();
            for (i, p) in self.properties.iter().enumerate() {
                l.props.insert(p.name.clone(), PropIndex(i as u32));
            }
            for (i, s) in self.signals.iter().enumerate() {
                l.signals.insert(s.name.clone(), SignalIndex(i as u32));
            }
            for (i, m) in self.methods.iter().enumerate() {
                l.methods.insert(m.name.clone(), MethodIndex(i as u32));
            }
            l
        })
    }

    /// A property by name, inherited ones included.
    pub fn property_index(&self, name: &str) -> Option<PropIndex> {
        self.lookup().props.get(name).copied()
    }

    pub fn property(&self, i: PropIndex) -> Option<&PropertyInfo> {
        self.properties.get(i.0 as usize)
    }

    pub fn signal_index(&self, name: &str) -> Option<SignalIndex> {
        self.lookup().signals.get(name).copied()
    }

    pub fn signal(&self, i: SignalIndex) -> Option<&SignalInfo> {
        self.signals.get(i.0 as usize)
    }

    pub fn method_index(&self, name: &str) -> Option<MethodIndex> {
        self.lookup().methods.get(name).copied()
    }

    pub fn method(&self, i: MethodIndex) -> Option<&MethodInfo> {
        self.methods.get(i.0 as usize)
    }

    /// The type's own properties, without inherited ones.
    pub fn own_properties(&self) -> &[PropertyInfo] {
        &self.properties[self.property_offset as usize..]
    }

    pub fn own_signals(&self) -> &[SignalInfo] {
        &self.signals[self.signal_offset as usize..]
    }

    pub fn own_methods(&self) -> &[MethodInfo] {
        &self.methods[self.method_offset as usize..]
    }

    /// The default property, inherited if the type sets none.
    pub fn default_property(&self) -> Option<&Atom> {
        self.default_property
            .as_ref()
            .or_else(|| self.base.as_ref()?.default_property())
    }

    /// This type and its bases, most derived first.
    pub fn chain(&self) -> impl Iterator<Item = &MetaObject> {
        std::iter::successors(Some(self), |m| m.base.as_deref())
    }

    /// Whether this type is `other` or derives from it.
    pub fn inherits(&self, other: &MetaObject) -> bool {
        self.chain().any(|m| std::ptr::eq(m, other))
    }

    /// Whether this type is, or derives from, a type called `name`.
    pub fn inherits_name(&self, name: &str) -> bool {
        self.chain().any(|m| m.name == name)
    }

    /// The value of an enum key (`Ready` of `Component.Status`), searching
    /// every enum of the type, inherited ones included. Later wins.
    pub fn enum_key(&self, key: &str) -> Option<i64> {
        self.enums
            .iter()
            .rev()
            .find_map(|e| e.keys.iter().find(|(k, _)| k == key).map(|(_, v)| *v))
    }
}

/// A typed handle to a native property: `Rectangle::RADIUS`. Generated by
/// the derive; the index is a constant, so reads and writes through it skip
/// the name lookup.
pub struct Prop<T> {
    pub index: PropIndex,
    /// The type that declares it. The engine checks the object derives
    /// from it before using the index.
    pub owner: fn() -> &'static MetaObject,
    _t: PhantomData<fn() -> T>,
}

impl<T> Prop<T> {
    pub const fn new(index: u32, owner: fn() -> &'static MetaObject) -> Self {
        Prop {
            index: PropIndex(index),
            owner,
            _t: PhantomData,
        }
    }
}

impl<T> Clone for Prop<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Prop<T> {}

impl<T> fmt::Debug for Prop<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Prop({}.{})", (self.owner)().name, self.index.0)
    }
}

/// A signal as `#[qml_impl]` declares it.
#[derive(Clone, Debug)]
pub struct SignalDecl {
    pub name: &'static str,
    pub params: Vec<(&'static str, TypeTag)>,
}

/// A method as `#[qml_impl]` declares it.
#[derive(Clone, Debug)]
pub struct MethodDecl {
    pub name: &'static str,
    pub params: Vec<(&'static str, TypeTag)>,
    pub ret: TypeTag,
    pub thunk: MethodThunk,
}

/// What `#[qml_impl]` contributes to a type's description.
#[derive(Clone, Debug, Default)]
pub struct ImplInfo {
    pub signals: Vec<SignalDecl>,
    pub methods: Vec<MethodDecl>,
    pub on_complete: Option<CompleteThunk>,
}

/// Implemented by `#[qml_impl]`.
pub trait QmlImplInfo {
    fn impl_info() -> ImplInfo;
}

/// Lets the derive pick up `#[qml_impl]` output without knowing whether the
/// type has an impl block: method resolution prefers [`ImplViaTrait`] (no
/// autoref) when the type implements [`QmlImplInfo`] and falls back to
/// [`ImplFallback`] otherwise.
#[derive(Debug)]
pub struct Probe<T>(PhantomData<T>);

impl<T> Probe<T> {
    pub const fn new() -> Self {
        Probe(PhantomData)
    }
}

impl<T> Default for Probe<T> {
    fn default() -> Self {
        Self::new()
    }
}

pub trait ImplViaTrait {
    fn qml_impl_info(&self) -> ImplInfo;
}

impl<T: QmlImplInfo> ImplViaTrait for Probe<T> {
    fn qml_impl_info(&self) -> ImplInfo {
        T::impl_info()
    }
}

pub trait ImplFallback {
    fn qml_impl_info(&self) -> ImplInfo {
        ImplInfo::default()
    }
}

impl<T> ImplFallback for &Probe<T> {}

/// Builds a [`MetaObject`]. The derive uses it for native types; the
/// compiler and runtime use it for types declared in QML.
///
/// Call order: `native`/`base` first, then members.
#[derive(Debug)]
pub struct MetaBuilder {
    meta: MetaObject,
}

impl MetaBuilder {
    pub fn new(name: &str) -> Self {
        MetaBuilder {
            meta: MetaObject {
                name: Atom::intern(name),
                module: None,
                base: None,
                native: None,
                properties: Vec::new(),
                signals: Vec::new(),
                methods: Vec::new(),
                property_offset: 0,
                signal_offset: 0,
                method_offset: 0,
                dynamic_slots: 0,
                default_property: None,
                attached: None,
                create: None,
                on_complete: None,
                enums: Vec::new(),
                lookup: OnceLock::new(),
            },
        }
    }

    /// The Rust struct behind the type.
    pub fn native<T: QmlObject>(mut self) -> Self {
        self.meta.native = Some(TypeId::of::<T>());
        self
    }

    pub fn module(mut self, uri: &str) -> Self {
        self.meta.module = Some(Atom::intern(uri));
        self
    }

    /// Inherits `base`'s members, which come first in every table, and its
    /// constructor.
    pub fn base(mut self, base: impl Into<MetaRef>) -> Self {
        let base = base.into();
        let m = &mut self.meta;
        m.properties = base.properties.clone();
        m.signals = base.signals.clone();
        m.methods = base.methods.clone();
        m.property_offset = m.properties.len() as u32;
        m.signal_offset = m.signals.len() as u32;
        m.method_offset = m.methods.len() as u32;
        m.dynamic_slots = base.dynamic_slots;
        m.create = base.create;
        m.enums = base.enums.clone();
        m.base = Some(base);
        self
    }

    fn push_property(
        &mut self,
        name: Atom,
        ty: TypeTag,
        flags: PropertyFlags,
        notify: bool,
        access: PropertyAccess,
    ) {
        let index = PropIndex(self.meta.properties.len() as u32);
        let notify = notify.then(|| {
            let s = SignalIndex(self.meta.signals.len() as u32);
            self.meta.signals.push(SignalInfo {
                name: Atom::intern(&format!("{name}Changed")),
                params: Vec::new(),
                notify_of: Some(index),
            });
            s
        });
        self.meta.properties.push(PropertyInfo {
            name,
            ty,
            flags,
            notify,
            access,
            list_append: None,
        });
    }

    /// A property stored in a field of the native struct set by
    /// [`MetaBuilder::native`]. `notify` adds a `nameChanged` signal.
    pub fn field(
        mut self,
        name: &str,
        ty: TypeTag,
        flags: PropertyFlags,
        notify: bool,
        get: Getter,
        set: Setter,
    ) -> Self {
        let owner = self
            .meta
            .native
            .expect("MetaBuilder::field needs MetaBuilder::native first");
        self.push_property(
            Atom::intern(name),
            ty,
            flags | PropertyFlags::FINAL,
            notify,
            PropertyAccess::Field { owner, get, set },
        );
        self
    }

    /// A grouped property: `name` itself plus `name.leaf` for every
    /// property of `group`.
    pub fn group(
        mut self,
        name: &str,
        group: &'static MetaObject,
        flags: PropertyFlags,
        get: GroupGet,
        get_mut: GroupGetMut,
    ) -> Self {
        let owner = self
            .meta
            .native
            .expect("MetaBuilder::group needs MetaBuilder::native first");
        self.push_property(
            Atom::intern(name),
            TypeTag::Group(group.name.clone()),
            flags | PropertyFlags::GROUP | PropertyFlags::FINAL | PropertyFlags::READONLY,
            false,
            PropertyAccess::GroupRoot,
        );
        for (i, leaf) in group.properties.iter().enumerate() {
            self.push_property(
                Atom::intern(&format!("{name}.{}", leaf.name)),
                leaf.ty.clone(),
                leaf.flags | PropertyFlags::FINAL,
                false,
                PropertyAccess::Group {
                    owner,
                    get,
                    get_mut,
                    meta: group,
                    leaf: PropIndex(i as u32),
                },
            );
        }
        self
    }

    /// A property declared in QML, stored in a dynamic slot.
    pub fn property(mut self, name: &str, ty: TypeTag, flags: PropertyFlags, notify: bool) -> Self {
        let slot = self.meta.dynamic_slots;
        self.meta.dynamic_slots += 1;
        self.push_property(
            Atom::intern(name),
            ty,
            flags,
            notify,
            PropertyAccess::Dynamic { slot },
        );
        self
    }

    pub fn signal(mut self, name: &str, params: Vec<(Atom, TypeTag)>) -> Self {
        self.meta.signals.push(SignalInfo {
            name: Atom::intern(name),
            params,
            notify_of: None,
        });
        self
    }

    pub fn method(
        mut self,
        name: &str,
        params: Vec<(Atom, TypeTag)>,
        ret: TypeTag,
        thunk: Option<MethodThunk>,
    ) -> Self {
        self.meta.methods.push(MethodInfo {
            name: Atom::intern(name),
            params,
            ret,
            thunk,
        });
        self
    }

    /// Routes appends to the property just added through `f`.
    pub fn list_append(mut self, f: ListAppend) -> Self {
        if let Some(p) = self.meta.properties.last_mut() {
            p.list_append = Some(f);
        }
        self
    }

    /// Marks an inherited property required (`required property` on a name
    /// a base declares).
    pub fn require(mut self, name: &str) -> Self {
        if let Some(p) = self
            .meta
            .properties
            .iter_mut()
            .rev()
            .find(|p| p.name == name)
        {
            p.flags |= PropertyFlags::REQUIRED;
        }
        self
    }

    /// Adds an enum.
    pub fn enumeration(mut self, name: &str, keys: &[(&str, i64)]) -> Self {
        self.meta.enums.push(EnumInfo {
            name: Atom::intern(name),
            keys: keys.iter().map(|(k, v)| (Atom::intern(k), *v)).collect(),
        });
        self
    }

    /// Adds several enums (what `#[qml(enums = f)]` passes).
    pub fn enumerations(mut self, enums: Vec<(&'static str, Vec<(&'static str, i64)>)>) -> Self {
        for (name, keys) in enums {
            self = self.enumeration(name, &keys);
        }
        self
    }

    pub fn default_property(mut self, name: &str) -> Self {
        self.meta.default_property = Some(Atom::intern(name));
        self
    }

    pub fn attached(mut self, f: fn() -> MetaRef) -> Self {
        self.meta.attached = Some(f);
        self
    }

    pub fn creatable(mut self, f: fn() -> Box<dyn QmlObject>) -> Self {
        self.meta.create = Some(f);
        self
    }

    /// Not creatable from QML, even if a base is.
    pub fn uncreatable(mut self) -> Self {
        self.meta.create = None;
        self
    }

    pub fn on_complete(mut self, f: CompleteThunk) -> Self {
        self.meta.on_complete = Some(f);
        self
    }

    /// Adds what `#[qml_impl]` declared.
    pub fn impl_info(mut self, info: ImplInfo) -> Self {
        let named = |params: Vec<(&'static str, TypeTag)>| {
            params
                .into_iter()
                .map(|(n, t)| (Atom::intern(n), t))
                .collect::<Vec<_>>()
        };
        for s in info.signals {
            self = self.signal(s.name, named(s.params));
        }
        for m in info.methods {
            self = self.method(m.name, named(m.params), m.ret, Some(m.thunk));
        }
        if let Some(f) = info.on_complete {
            self.meta.on_complete = Some(f);
        }
        self
    }

    pub fn build(self) -> MetaObject {
        self.meta
    }

    /// Builds a reference-counted description (types declared in QML).
    pub fn build_shared(self) -> MetaRef {
        MetaRef::Shared(Arc::new(self.meta))
    }
}
