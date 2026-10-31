//! The structs, the faces they are told to and the tellings on their way.

use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::rc::Rc;

use js_sys::{Array, Function, Reflect};
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;

use super::item::ItemPaint;
use super::value::QmlValue;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &JsValue);
    #[wasm_bindgen(js_namespace = console, js_name = warn)]
    pub(super) fn console_warn(message: &str);
}

/// The handle an application's `register` is given. All of its state is the
/// thread's, as the page has the one engine.
#[derive(Clone, Copy, Debug, Default)]
pub struct Engine;

/// What a hook's `&Engine` is.
pub static ENGINE: Engine = Engine;

/// An object: a struct of a native type, or any other object of the runtime
/// a struct was given.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObjectId(pub(super) u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PropIndex(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SignalIndex(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
}

impl Version {
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }
}

/// An interned string natively; here only a shared one.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Atom(Rc<str>);

impl Default for Atom {
    fn default() -> Self {
        Self::from("")
    }
}

impl From<&str> for Atom {
    fn from(s: &str) -> Self {
        Self(Rc::from(s))
    }
}

impl Deref for Atom {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Atom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConvertError {
    expected: &'static str,
}

impl ConvertError {
    pub fn new(expected: &'static str) -> Self {
        Self { expected }
    }
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cannot convert to {}", self.expected)
    }
}

impl std::error::Error for ConvertError {}

/// As the engine's, without what only the engine can go wrong with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineError {
    DeadObject(ObjectId),
    WrongType { ty: String, expected: String },
    NotCreatable(String),
    Convert(ConvertError),
    Invoke(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeadObject(id) => write!(f, "object {id:?} has been destroyed"),
            Self::WrongType { ty, expected } => write!(f, "{ty} is not a {expected}"),
            Self::NotCreatable(ty) => write!(f, "{ty} is not creatable"),
            Self::Convert(e) => e.fmt(f),
            Self::Invoke(said) => f.write_str(said),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<ConvertError> for EngineError {
    fn from(e: ConvertError) -> Self {
        Self::Convert(e)
    }
}

impl From<EngineError> for JsValue {
    fn from(e: EngineError) -> Self {
        js_sys::Error::new(&e.to_string()).into()
    }
}

/// A derived type. `web_read` is a telling's value by the property's number.
pub trait QmlType: Any {
    const NAME: &'static str;
    fn web_read(&self, _index: u32) -> JsValue {
        JsValue::UNDEFINED
    }
}

/// What a native type can build on: a base's properties are slots of the
/// runtime's type, and there is no face to split between two wrappers.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be the base of a native type on the web",
    note = "only `Item` and `QtObject` can: a native type on another native type has no face yet (docs/LJOS_WEB.md A22)"
)]
pub trait WebBase {}
impl WebBase for super::QtObject {}
impl WebBase for super::item::Item {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SingletonScope {
    Engine,
    Window,
}

pub trait QmlSingleton: QmlType {
    const SCOPE: SingletonScope;
}

#[derive(Debug)]
pub struct ModuleDef;

impl ModuleDef {
    pub fn new(_uri: &str, _version: &str, _types: &[&str]) -> Self {
        Self
    }
}

/// A typed handle, `Gauge::VALUE`. One with no `get` is a slot of the face:
/// a base's property, or a group.
type Getter<T> = fn(&Cx<'_>) -> Result<T, EngineError>;
type Setter<T> = fn(&Cx<'_>, T) -> Result<(), EngineError>;

#[derive(Debug)]
pub struct Prop<T: 'static> {
    pub(super) name: &'static str,
    index: u32,
    pub(super) get: Option<Getter<T>>,
    pub(super) set: Option<Setter<T>>,
    _t: PhantomData<fn() -> T>,
}

impl<T> Clone for Prop<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Prop<T> {}

impl<T> Prop<T> {
    pub const fn own(
        name: &'static str,
        index: u32,
        get: fn(&Cx<'_>) -> Result<T, EngineError>,
        set: fn(&Cx<'_>, T) -> Result<(), EngineError>,
    ) -> Self {
        Self {
            name,
            index,
            get: Some(get),
            set: Some(set),
            _t: PhantomData,
        }
    }

    pub const fn slot(name: &'static str, index: u32) -> Self {
        Self {
            name,
            index,
            get: None,
            set: None,
            _t: PhantomData,
        }
    }

    pub const fn index(&self) -> PropIndex {
        PropIndex(self.index)
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }
}

type ReadOf = fn(&Rc<dyn Any>, u32) -> JsValue;

struct Slot {
    data: Option<Rc<dyn Any>>,
    ty: TypeId,
    ty_name: &'static str,
    read: Option<ReadOf>,
    face: Option<JsValue>,
    listener: Option<Function>,
    pub(super) paint: Option<Rc<dyn ItemPaint>>,
}

#[derive(Default)]
struct Store {
    slots: Vec<Option<Slot>>,
    free: Vec<u32>,
}

type Make = Rc<dyn Fn() -> Rc<dyn Any>>;

thread_local! {
    static STORE: RefCell<Store> = RefCell::default();
    static TELLS: RefCell<Tells> = RefCell::default();
    static FACTORIES: RefCell<HashMap<TypeId, Make>> = RefCell::default();
    // Inside a paint item's `read`, a read goes to the face so that the
    // runtime's tracking sees it.
    static TRACKING: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn tracking<R>(work: impl FnOnce() -> R) -> R {
    TRACKING.with(|t| t.set(true));
    let r = work();
    TRACKING.with(|t| t.set(false));
    r
}

fn insert(slot: Slot) -> ObjectId {
    STORE.with_borrow_mut(|store| {
        if let Some(at) = store.free.pop() {
            store.slots[at as usize] = Some(slot);
            ObjectId(at)
        } else {
            store.slots.push(Some(slot));
            ObjectId(store.slots.len() as u32 - 1)
        }
    })
}

fn at<R>(id: ObjectId, f: impl FnOnce(&mut Slot) -> R) -> Option<R> {
    STORE.with_borrow_mut(|store| store.slots.get_mut(id.0 as usize)?.as_mut().map(f))
}

pub(super) fn set_paint(id: ObjectId, paint: Rc<dyn ItemPaint>) {
    at(id, |slot| slot.paint = Some(paint));
}

pub(super) fn paint_of(id: ObjectId) -> Option<Rc<dyn ItemPaint>> {
    at(id, |slot| slot.paint.clone()).flatten()
}

pub(super) fn type_of(id: ObjectId) -> Option<TypeId> {
    at(id, |slot| slot.ty)
}

fn reader<T: QmlType>(data: &Rc<dyn Any>, index: u32) -> JsValue {
    match data.downcast_ref::<RefCell<T>>().map(RefCell::try_borrow) {
        Some(Ok(this)) => this.web_read(index),
        _ => JsValue::UNDEFINED,
    }
}

/// What a wrapper's constructor does: the registered maker if there is one,
/// which is a singleton's or a hand-off's, else what a type is made with.
/// A type's `Default` if it has one. The inherent `get` wins where the bound
/// holds and the trait's answers where it doesn't, which a macro can ask of a
/// named type without knowing which it is.
#[derive(Debug)]
pub struct DefaultOf<T>(std::marker::PhantomData<T>);

impl<T> DefaultOf<T> {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<T: Default> DefaultOf<T> {
    pub fn get(&self) -> Option<fn() -> T> {
        Some(T::default)
    }
}

pub trait NoDefault<T> {
    fn get(&self) -> Option<fn() -> T> {
        None
    }
}

impl<T> NoDefault<T> for DefaultOf<T> {}

pub fn make<T: QmlType>(default: Option<fn() -> T>) -> Result<ObjectId, EngineError> {
    let made = FACTORIES.with_borrow(|f| f.get(&TypeId::of::<T>()).cloned());
    let data: Rc<dyn Any> = match (made, default) {
        (Some(make), _) => make(),
        (None, Some(default)) => Rc::new(RefCell::new(default())),
        (None, None) => return Err(EngineError::NotCreatable(T::NAME.to_owned())),
    };
    Ok(insert_data::<T>(data))
}

fn insert_data<T: QmlType>(data: Rc<dyn Any>) -> ObjectId {
    insert(Slot {
        data: Some(data),
        ty: TypeId::of::<T>(),
        ty_name: T::NAME,
        read: Some(reader::<T>),
        face: None,
        listener: None,
        paint: None,
    })
}

/// A wrapper was dropped: the object went with its face.
pub fn free(id: ObjectId) {
    STORE.with_borrow_mut(|store| {
        if let Some(slot) = store.slots.get_mut(id.0 as usize) {
            *slot = None;
            store.free.push(id.0);
        }
    });
}

/// The id a wrapper's constructor returned, as JavaScript holds it.
pub fn self_id(id: ObjectId) -> u32 {
    id.0
}

pub fn by_id(id: u32) -> ObjectId {
    ObjectId(id)
}

const ID_KEY: &str = "$ljosId";

/// Who the struct's face is: what an `ObjectId` property reads as.
pub fn bind(id: ObjectId, face: JsValue) {
    let _ = Reflect::set(&face, &JsValue::from_str(ID_KEY), &JsValue::from(id.0));
    at(id, |slot| slot.face = Some(face));
}

pub fn listen(id: ObjectId, listener: Function) {
    at(id, |slot| slot.listener = Some(listener));
}

pub(super) fn by_ref_face(id: ObjectId, method: &str) -> Option<(JsValue, Function)> {
    let face = face_of(id)?;
    let f = Reflect::get(&face, &JsValue::from_str(method)).ok()?;
    f.is_function().then(|| (face, f.into()))
}

pub(super) fn face_of(id: ObjectId) -> Option<JsValue> {
    at(id, |slot| slot.face.clone()).flatten()
}

/// The id of whatever object JavaScript hands over. One with no struct, an
/// `Item` of the runtime's, gets an id for what is read of it through its slots.
pub(super) fn id_of(value: &JsValue) -> Option<ObjectId> {
    if !value.is_object() {
        return None;
    }
    if let Some(id) = Reflect::get(value, &JsValue::from_str(ID_KEY))
        .ok()
        .and_then(|v| v.as_f64())
    {
        return Some(ObjectId(id as u32));
    }
    let id = insert(Slot {
        data: None,
        ty: TypeId::of::<()>(),
        ty_name: "object",
        read: None,
        face: Some(value.clone()),
        listener: None,
        paint: None,
    });
    let _ = Reflect::set(value, &JsValue::from_str(ID_KEY), &JsValue::from(id.0));
    Some(id)
}

fn lent<T: 'static>(id: ObjectId) -> Result<Rc<RefCell<T>>, EngineError> {
    let (data, name) =
        at(id, |slot| (slot.data.clone(), slot.ty_name)).ok_or(EngineError::DeadObject(id))?;
    data.and_then(|d| d.downcast::<RefCell<T>>().ok())
        .ok_or_else(|| EngineError::WrongType {
            ty: name.to_owned(),
            expected: std::any::type_name::<T>().to_owned(),
        })
}

/// A call into Rust from outside. The outermost's end is when what happened
/// in it is told, so a struct is never lent while a handler runs.
#[derive(Debug)]
pub struct Call(());

impl Call {
    pub fn enter() -> Self {
        TELLS.with_borrow_mut(|t| t.depth += 1);
        Self(())
    }
}

impl Drop for Call {
    fn drop(&mut self) {
        let outermost = TELLS.with_borrow_mut(|t| {
            t.depth -= 1;
            t.depth == 0
        });
        if outermost {
            flush();
        }
    }
}

#[derive(Default)]
struct Told {
    properties: Vec<(u32, JsValue)>,
    signals: Vec<(&'static str, JsValue)>,
}

#[derive(Default)]
struct Tells {
    depth: u32,
    pending: Vec<(ObjectId, Told)>,
}

fn told<R>(id: ObjectId, f: impl FnOnce(&mut Told) -> R) -> R {
    TELLS.with_borrow_mut(|t| {
        let at = t.pending.iter().position(|(who, _)| *who == id);
        let at = at.unwrap_or_else(|| {
            t.pending.push((id, Told::default()));
            t.pending.len() - 1
        });
        f(&mut t.pending[at].1)
    })
}

fn has_listener(id: ObjectId) -> bool {
    at(id, |slot| slot.listener.is_some()).unwrap_or(false)
}

/// A property changed. A change told twice in one call is told once, with
/// where it ended; a group is told whole.
pub fn tell(id: ObjectId, index: u32, value: JsValue) {
    if !has_listener(id) {
        return;
    }
    let _call = Call::enter();
    told(id, |t| {
        match t.properties.iter_mut().find(|(k, _)| *k == index) {
            Some(slot) => slot.1 = value,
            None => t.properties.push((index, value)),
        }
    });
}

/// A signal: told after the properties of the call that emitted it.
pub fn emit(cx: &Cx<'_>, name: &'static str, args: &[JsValue]) -> Result<(), EngineError> {
    if !has_listener(cx.this) {
        return Ok(());
    }
    let _call = Call::enter();
    let args: Array = args.iter().collect();
    told(cx.this, |t| t.signals.push((name, args.into())));
    Ok(())
}

fn flush() {
    // A handler may call back in: the queue is out of the way by then, and
    // that call is an outermost one that tells of itself before it returns.
    while let Some(work) =
        TELLS.with_borrow_mut(|t| (!t.pending.is_empty()).then(|| std::mem::take(&mut t.pending)))
    {
        for (id, told) in work {
            let Some(listener) = at(id, |slot| slot.listener.clone()).flatten() else {
                continue;
            };
            let flat = Array::new();
            for (index, value) in told.properties {
                flat.push(&JsValue::from(index));
                flat.push(&value);
            }
            for (name, args) in told.signals {
                flat.push(&JsValue::from_str(name));
                flat.push(&args);
            }
            if let Err(error) = listener.call1(&JsValue::NULL, &flat) {
                console_error(&error);
            }
        }
    }
}

/// What a hook or an invokable works through, as natively.
#[derive(Clone, Copy, Debug)]
pub struct Cx<'e> {
    pub engine: &'e Engine,
    pub this: ObjectId,
}

impl<'e> Cx<'e> {
    pub fn new(engine: &'e Engine, this: ObjectId) -> Self {
        Self { engine, this }
    }

    /// Lends the struct for the closure.
    pub fn with<T: 'static, R>(&self, f: impl FnOnce(&T) -> R) -> Result<R, EngineError> {
        self.engine.with(self.this, f)
    }

    /// Lends the struct, mutably, for the closure.
    pub fn with_mut<T: 'static, R>(&self, f: impl FnOnce(&mut T) -> R) -> Result<R, EngineError> {
        self.engine.with_mut(self.this, f)
    }

    pub fn get<T: QmlValue>(&self, p: Prop<T>) -> Result<T, EngineError> {
        self.engine.get(self.this, p)
    }

    pub fn set<T: QmlValue>(&self, p: Prop<T>, v: T) -> Result<(), EngineError> {
        self.engine.set(self.this, p, v)
    }

    /// Tells of a property a `with_mut` changed behind its setter.
    pub fn notify(&self, index: PropIndex) {
        let value = at(self.this, |slot| {
            let read = slot.read?;
            Some(read(slot.data.as_ref()?, index.0))
        })
        .flatten();
        if let Some(value) = value {
            tell(self.this, index.0, value);
        }
    }
}

impl Engine {
    pub fn with<T: 'static, R>(
        &self,
        obj: ObjectId,
        f: impl FnOnce(&T) -> R,
    ) -> Result<R, EngineError> {
        let data = lent::<T>(obj)?;
        let this = data
            .try_borrow()
            .map_err(|_| EngineError::Invoke("the object is being changed".to_owned()))?;
        Ok(f(&this))
    }

    pub fn with_mut<T: 'static, R>(
        &self,
        obj: ObjectId,
        f: impl FnOnce(&mut T) -> R,
    ) -> Result<R, EngineError> {
        let data = lent::<T>(obj)?;
        let mut this = data
            .try_borrow_mut()
            .map_err(|_| EngineError::Invoke("the object is lent".to_owned()))?;
        Ok(f(&mut this))
    }

    /// An own property reads the struct, which holds it. Inside a paint
    /// item's `read`, or for a base's, it reads the face.
    pub fn get<T: QmlValue>(&self, obj: ObjectId, p: Prop<T>) -> Result<T, EngineError> {
        let face = (TRACKING.with(Cell::get) || p.get.is_none())
            .then(|| face_of(obj))
            .flatten();
        match (face, p.get) {
            (Some(face), _) => {
                let value = Reflect::get(&face, &JsValue::from_str(p.name))
                    .map_err(|_| EngineError::DeadObject(obj))?;
                Ok(T::from_js(&value)?)
            }
            (None, Some(get)) => get(&Cx::new(self, obj)),
            (None, None) => Err(EngineError::DeadObject(obj)),
        }
    }

    pub fn set<T: QmlValue>(&self, obj: ObjectId, p: Prop<T>, v: T) -> Result<(), EngineError> {
        let _call = Call::enter();
        if let Some(set) = p.set {
            return set(&Cx::new(self, obj), v);
        }
        let face = face_of(obj).ok_or(EngineError::DeadObject(obj))?;
        Reflect::set(&face, &JsValue::from_str(p.name), &v.to_js())
            .map_err(|_| EngineError::DeadObject(obj))?;
        Ok(())
    }

    pub fn register_module(&self, _def: ModuleDef) {}

    pub fn register_singleton<T: QmlSingleton + Default>(&self, _uri: &str, _version: Version) {}

    /// A singleton made by a closure: the generated constructor asks here.
    pub fn register_singleton_with<T: QmlSingleton>(
        &self,
        _uri: &str,
        _version: Version,
        make: impl Fn(&Engine) -> T + 'static,
    ) {
        let make: Make = Rc::new(move || Rc::new(RefCell::new(make(&ENGINE))));
        FACTORIES.with_borrow_mut(|f| f.insert(TypeId::of::<T>(), make));
    }

    /// An uncreatable type's object, made in Rust and handed to QML: its face
    /// is made as it is first read.
    pub fn hand_off<T: QmlType>(&self, value: T) -> ObjectId {
        insert_data::<T>(Rc::new(RefCell::new(value)))
    }
}

/// A type's named fields, for the group it may be of another.
pub trait WebFields {
    fn web_get(&self, name: &str) -> Option<JsValue>;
    /// Whether it changed.
    fn web_set(&mut self, name: &str, value: &JsValue) -> Result<bool, EngineError>;
    /// All of them, as one object: how a group is told.
    fn web_all(&self) -> JsValue;
}

/// What a type's `enums = path` gives, as the face reads it once.
pub fn enums_to_js(enums: Vec<(&'static str, Vec<(&'static str, i64)>)>) -> JsValue {
    let out = Array::new();
    for (name, keys) in enums {
        let entry = Array::new();
        entry.push(&JsValue::from_str(name));
        for (key, value) in keys {
            entry.push(&JsValue::from_str(key));
            entry.push(&JsValue::from_f64(value as f64));
        }
        out.push(&entry);
    }
    out.into()
}

/// Natively `#[qml(write = path)]` is either of two signatures, and the
/// method's picks which; so here.
pub trait WriteFn<S, T, M> {
    fn call(&self, cx: &Cx<'_>, value: T) -> Result<Option<T>, EngineError>;
}

#[derive(Debug)]
pub struct ByRef;
#[derive(Debug)]
pub struct ByCx;

impl<S: 'static, T, F: Fn(&S, T) -> Option<T>> WriteFn<S, T, ByRef> for F {
    fn call(&self, cx: &Cx<'_>, value: T) -> Result<Option<T>, EngineError> {
        cx.with::<S, _>(|this| self(this, value))
    }
}

impl<S, T, F: Fn(&Cx<'_>, T) -> Option<T>> WriteFn<S, T, ByCx> for F {
    fn call(&self, cx: &Cx<'_>, value: T) -> Result<Option<T>, EngineError> {
        Ok(self(cx, value))
    }
}

/// Which hooks an object's type has: a bit each, what the face installs.
pub fn hooks_of(id: ObjectId) -> u32 {
    super::item::hooks_bits(id)
}
