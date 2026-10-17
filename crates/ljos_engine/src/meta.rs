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
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::{Arc, OnceLock};

use rustc_hash::FxHashMap;

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
/// Decides what a write to a native field stores, before it is stored or
/// heard: `Some` is the value to store (the one written, or a corrected
/// one), `None` refuses the write. It sees every write, one of the value
/// the field already holds too. The value is already of the property's
/// type.
#[derive(Clone, Copy, Debug)]
pub enum WriteFilter {
    /// Gets the struct that declares the field (`fn(&self, T)`).
    Native(fn(&dyn QmlObject, &Value) -> Result<Option<Value>, ConvertError>),
    /// Gets the engine and the object (`fn(cx: &Cx, T)`), and runs with
    /// nothing borrowed, so it may read and write the object's other
    /// properties, as a WRITE function in Qt may. What it writes goes
    /// through their own filters and is heard before the write that ran
    /// it is stored. A write of the property it filters is refused
    /// ([`EngineError::WriteInOwnSetter`]); its reads are not tracked.
    Cx(CxWrite),
}
/// A [`WriteFilter::Cx`] setter.
pub type CxWrite = fn(&Cx<'_>, &Value) -> Result<Option<Value>, ConvertError>;
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
/// Gets a native list field from the struct that declares it, to change it
/// where it is: an append then costs the same however long the list is.
pub type ListField = fn(&mut dyn QmlObject) -> &mut crate::list::QmlList;

/// What a property holds while nothing has given it a value: `engine`, the
/// object and the property in, the value out. It runs as part of a binding,
/// so what it reads is followed.
pub type ResetValue = fn(&crate::Engine, crate::ids::ObjectId, PropIndex) -> Value;
/// Takes a property back to the state it had before anything gave it a
/// value.
pub type ResetApply =
    fn(&crate::Engine, crate::ids::ObjectId, PropIndex) -> Result<(), EngineError>;

/// How a property is taken back, which is what assigning `undefined` to it
/// does (`width = undefined`), also when a binding on it yields
/// `undefined`. Qt calls it the property's reset; a property without one
/// refuses `undefined`.
#[derive(Clone, Copy, Debug)]
pub enum Reset {
    /// The value the type gives a new object, and the property counts as
    /// never set ([`Provenance::Default`](crate::Provenance::Default)).
    Default,
    /// The property follows the value this gives until something else is
    /// assigned: a side margin follows the margins.
    Follow(ResetValue),
    /// [`Reset::Follow`], and a new object holds what `value` gives before
    /// any of its bindings runs: a popup's `parent` is the item it is
    /// declared in when its own `width: parent.width` first runs.
    FollowFromCreation(ResetValue),
    /// The type does it itself. `value` is what the property holds then,
    /// for a binding that yields `undefined` and stays in place.
    Custom {
        apply: ResetApply,
        value: ResetValue,
    },
    /// [`Reset::Custom`], and a new object holds what `value` gives before
    /// any of its bindings runs, as with [`Reset::FollowFromCreation`]: a
    /// control's `locale` is a locale while the control is being made.
    CustomFromCreation {
        apply: ResetApply,
        value: ResetValue,
    },
    /// The property holds what `value` gives while it was given nothing,
    /// and a value `absent` says yes to counts as nothing given: an empty
    /// `baseUrl` is the document's address. `value` is asked once each
    /// time; the property does not follow it.
    Absent {
        absent: fn(&Value) -> bool,
        value: ResetValue,
    },
}

impl Reset {
    /// Whether assigning `v` takes the property back.
    pub(crate) fn takes_back(&self, v: &Value) -> bool {
        match self {
            _ if matches!(v, Value::Undefined) => true,
            Reset::Absent { absent, .. } => absent(v),
            _ => false,
        }
    }
}

/// Where a property's value is stored.
#[derive(Clone, Copy)]
pub enum PropertyAccess {
    /// A field of the native struct `owner`, which is the object's native or
    /// one of its bases.
    Field {
        owner: TypeId,
        get: Getter,
        set: Setter,
        /// What a write goes through first ([`MetaBuilder::write`]).
        write: Option<WriteFilter>,
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
    /// For a native list property, the field itself.
    pub list_field: Option<ListField>,
    /// How `undefined` takes the property back, if it can be.
    pub reset: Option<Reset>,
    /// For an object property that takes any object, the class a
    /// description of the type says it holds: a control's part holds an
    /// `Item` once made, though a style gives it as a `Component` first.
    pub holds: Option<Atom>,
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
    props: FxHashMap<Atom, PropIndex>,
    signals: FxHashMap<Atom, SignalIndex>,
    methods: FxHashMap<Atom, MethodIndex>,
    /// The properties with a [`Reset::Absent`] or a
    /// [`Reset::FollowFromCreation`], and what gives each its first value.
    /// Empty for nearly every type.
    absent: Vec<(PropIndex, ResetValue)>,
}

/// What in QML declares a type: an object prints by it (`String(item)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Declared {
    /// The root of a file: the type the file is.
    File,
    /// The root of an inline component.
    Inline,
    /// An object that declares members of its own.
    Object,
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
    /// What declared the type, for a type declared in QML.
    pub declared: Option<Declared>,
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
    /// Where this type's own `required property` declarations are
    /// written: `url:line:column: `, by property index.
    pub required_at: Vec<(PropIndex, Atom)>,
    /// Scripts see a plain object, not a QObject: it prints as one.
    pub plain_object: bool,
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
                if let Some(
                    Reset::Absent { value, .. }
                    | Reset::FollowFromCreation(value)
                    | Reset::CustomFromCreation { value, .. },
                ) = p.reset
                {
                    l.absent.push((PropIndex(i as u32), value));
                }
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

    /// The properties that hold what a function gives until they are given
    /// a value ([`Reset::Absent`]), inherited ones included.
    pub(crate) fn absent(&self) -> &[(PropIndex, ResetValue)] {
        &self.lookup().absent
    }

    /// A property by name, inherited ones included.
    pub fn property_index(&self, name: &str) -> Option<PropIndex> {
        self.lookup().props.get(name).copied()
    }

    pub fn property(&self, i: PropIndex) -> Option<&PropertyInfo> {
        self.properties.get(i.0 as usize)
    }

    /// Where the `required property` `i` is declared, in this type or the
    /// one that declares it: `url:line:column: `.
    pub fn required_at(&self, i: PropIndex) -> Option<&Atom> {
        let mut m = self;
        loop {
            if i.0 >= m.property_offset {
                return m
                    .required_at
                    .iter()
                    .find(|(p, _)| *p == i)
                    .map(|(_, at)| at);
            }
            m = m.base.as_ref()?;
        }
    }

    pub fn signal_index(&self, name: &str) -> Option<SignalIndex> {
        self.lookup().signals.get(name).copied()
    }

    pub fn signal(&self, i: SignalIndex) -> Option<&SignalInfo> {
        self.signals.get(i.0 as usize)
    }

    /// Whether the signal was declared in QML (`signal sent(var list)`)
    /// and not by a native type.
    pub fn signal_is_script(&self, i: SignalIndex) -> bool {
        let mut m = self;
        loop {
            if m.native.is_some() {
                return false;
            }
            if i.0 >= m.signal_offset {
                return m
                    .signals
                    .get(i.0 as usize)
                    .is_some_and(|s| s.notify_of.is_none());
            }
            match &m.base {
                Some(b) => m = b,
                None => return false,
            }
        }
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

    /// The enum called `name`, the type's own or an inherited one. Later
    /// wins.
    pub fn enumeration(&self, name: &str) -> Option<&EnumInfo> {
        self.enums.iter().rev().find(|e| e.name == name)
    }

    /// Converts `v` for a property that holds the enum `name` of this type
    /// (see [`TypeTag::Enum`]): a number as it is (a real is rounded), a
    /// bool as 0 or 1, and a string when it is one of the enum's keys. The
    /// result is the key's number, a [`Value::Int`]: an enum reads as a
    /// number in QML.
    ///
    /// A number that is no key is taken as it is, as Qt takes it. What Qt
    /// then keeps of it differs from one property to the next (some store
    /// a few bits), which is not copied here.
    ///
    /// `name` with `Qt.` in front is an enum of the `Qt` namespace, which
    /// no type declares: `Qt.LayoutDirection`.
    pub fn coerce_enum(&self, name: &str, v: &Value) -> Result<Value, ConvertError> {
        self.coerce_keys(name, v, false)
    }

    /// As [`coerce_enum`](Self::coerce_enum) for a property that holds
    /// flags (see [`TypeTag::Flags`]): a string may name several keys with
    /// `|` between them, and is then their numbers combined. Spaces around
    /// a key are not part of it.
    pub fn coerce_flags(&self, name: &str, v: &Value) -> Result<Value, ConvertError> {
        self.coerce_keys(name, v, true)
    }

    fn coerce_keys(&self, name: &str, v: &Value, flags: bool) -> Result<Value, ConvertError> {
        let whole = match v {
            Value::Int(i) => Some(*i),
            Value::Enum(e) => Some(e.value),
            Value::Bool(b) => Some(i32::from(*b)),
            // Half away from zero; past the range it wraps, as `int` does.
            Value::Real(r) if !r.is_nan() => Some(r.round() as i64 as i32),
            Value::String(text) => {
                let number = if flags {
                    text.split('|')
                        .map(|key| self.key_of(name, key.trim()))
                        .try_fold(0, |all, one| Some(all | one?))
                } else {
                    self.key_of(name, text)
                };
                number.and_then(|n| i32::try_from(n).ok())
            }
            _ => None,
        };
        whole
            .map(Value::Int)
            .ok_or_else(|| ConvertError::new(crate::types::describe(v), self.enum_name(name)))
    }

    /// The number of `key` in the enum `name`.
    fn key_of(&self, name: &str, key: &str) -> Option<i64> {
        let keys = match name.strip_prefix("Qt.") {
            Some(of_qt) => {
                let found = crate::qt_enums::qt_enum(of_qt)?;
                return found.keys.iter().find(|(k, _)| *k == key).map(|(_, n)| *n);
            }
            None => &self.held_enumeration(name)?.keys,
        };
        keys.iter().find(|(k, _)| k == key).map(|(_, n)| *n)
    }

    /// The enum a property of this type is declared with: one of the
    /// type's, or for the member of a group (`easing.type`) one of the
    /// group's type, whose members are this type's properties but whose
    /// enums are not this type's (`Easing.InQuad`, not
    /// `NumberAnimation.InQuad`). The type's own wins.
    fn held_enumeration(&self, name: &str) -> Option<&EnumInfo> {
        self.enumeration(name).or_else(|| {
            self.properties.iter().find_map(|p| match p.access {
                PropertyAccess::Group { meta, .. } => meta.enumeration(name),
                _ => None,
            })
        })
    }

    /// Whether the enum a property is declared with exists: one of this
    /// type's, or with `Qt.` in front one of the `Qt` namespace's.
    pub fn has_enumeration(&self, name: &str) -> bool {
        match name.strip_prefix("Qt.") {
            Some(of_qt) => crate::qt_enums::qt_enum(of_qt).is_some(),
            None => self.held_enumeration(name).is_some(),
        }
    }

    /// An enum as messages name it: `Item::TransformOrigin`, with the type
    /// that declares it.
    fn enum_name(&self, name: &str) -> String {
        if let Some(of_qt) = name.strip_prefix("Qt.") {
            return format!("Qt::{of_qt}");
        }
        let has = |m: &MetaObject| m.enums.iter().any(|e| e.name == name);
        // Enums are inherited, so the last type of the chain to have it is
        // where it was declared.
        if let Some(owner) = self.chain().filter(|m| has(m)).last() {
            return format!("{}::{name}", owner.name);
        }
        let of_group = self.properties.iter().find_map(|p| match p.access {
            PropertyAccess::Group { meta, .. } if has(meta) => Some(meta),
            _ => None,
        });
        match of_group {
            Some(group) => format!("{}::{name}", group.name),
            None => name.to_owned(),
        }
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
                declared: None,
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
                required_at: Vec::new(),
                plain_object: false,
                lookup: OnceLock::new(),
            },
        }
    }

    /// The Rust struct behind the type.
    pub fn native<T: QmlObject>(mut self) -> Self {
        self.meta.native = Some(TypeId::of::<T>());
        self
    }

    /// What in QML declares the type.
    pub fn declared(mut self, by: Declared) -> Self {
        self.meta.declared = Some(by);
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
            list_field: None,
            reset: None,
            holds: None,
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
            PropertyAccess::Field {
                owner,
                get,
                set,
                write: None,
            },
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
            // The leaf's reset is the member's: it is given the object that
            // holds the group and the member's index there.
            if let Some(p) = self.meta.properties.last_mut() {
                p.reset = leaf.reset;
            }
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

    /// Sends writes to the field just added through `f` before they are
    /// stored, so a type can clamp or refuse a value, or change other
    /// properties, where Qt's setter would: nothing hears a write `f`
    /// corrects to the value already held, nor one it refuses. QML
    /// assignments, bindings, animation steps and writes from Rust all go
    /// through it; taking the property back with `undefined` does not.
    pub fn write(mut self, f: WriteFilter) -> Self {
        if let Some(PropertyAccess::Field { write, .. }) =
            self.meta.properties.last_mut().map(|p| &mut p.access)
        {
            *write = Some(f);
        }
        self
    }

    /// Says what class of object the property just added is described as
    /// holding ([`PropertyInfo::holds`]).
    pub fn holds(mut self, class: &str) -> Self {
        if let Some(p) = self.meta.properties.last_mut() {
            p.holds = Some(Atom::intern(class));
        }
        self
    }

    /// Says how assigning `undefined` takes the property just added back.
    pub fn reset(mut self, reset: Reset) -> Self {
        if let Some(p) = self.meta.properties.last_mut() {
            p.reset = Some(reset);
        }
        self
    }

    /// Says how assigning `undefined` takes back a property a base
    /// declares and cannot take back itself: an item only stores its
    /// `baselineOffset`, a control works one out.
    pub fn reset_inherited(mut self, name: &str, reset: Reset) -> Self {
        if let Some(p) = self
            .meta
            .properties
            .iter_mut()
            .rev()
            .find(|p| p.name == name)
        {
            p.reset = Some(reset);
        }
        self
    }

    /// Says where the list property just added keeps its items.
    pub fn list_field(mut self, f: ListField) -> Self {
        if let Some(p) = self.meta.properties.last_mut() {
            p.list_field = Some(f);
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

    /// Notes where the `required property` just added, the last property,
    /// is declared: `at` is `url:line:column: `.
    pub fn required_at(mut self, at: Atom) -> Self {
        if let Some(last) = self.meta.properties.len().checked_sub(1) {
            self.meta.required_at.push((PropIndex(last as u32), at));
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

    /// Scripts see a plain object, not a QObject.
    pub fn plain_object(mut self) -> Self {
        self.meta.plain_object = true;
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
