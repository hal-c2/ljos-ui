//! `PropertyMap`: key and value pairs given to QML by the application,
//! the equivalent of Qt's `QQmlPropertyMap`
//! (https://doc.qt.io/qt-6/qqmlpropertymap.html, ARCHITECTURE.md §17).
//!
//! Qt has no QML type for it (`PropertyMap { }` is rejected under every
//! import, measured on Qt 6.11.2): application code makes a map, gives
//! it its keys, and hands the object to QML, in a context or a property.
//! Each key is then a property of the object with its own change
//! notification, so a binding that reads `owner.name` follows exactly
//! that key. Writing a key in QML emits `valueChanged(key, value)`,
//! which is how the application hears of it; the application's own
//! [`PropertyMap::insert`] and [`PropertyMap::clear`] do not emit it.
//! Keys are never taken away: `clear` leaves the key reading
//! `undefined` in QML and still listed by `keys()`.
//!
//! Keys appear as dynamic properties widened onto the object's type in
//! place, as the row objects of `model.rs` gain roles. Two things Qt
//! does are not built yet, because the engine has no hook for a read or
//! a write of a property an object has not got: reading a key the map
//! has not got does not add it, and QML cannot add a key by writing it.
//! So give the map its keys before QML binds to them (the shape of
//! Qt's own example); a binding that reads a key first and is given it
//! later does not re-run. Both are scenarios in
//! `features/models/property-map.feature`, held at `@backlog`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::atom::Atom;
use crate::builtins::QtObject;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{ConnectionId, ObjectId, PropIndex};
use crate::meta::{MetaBuilder, MetaRef, PropertyFlags};
use crate::object::QmlType;
use crate::types::TypeTag;
use crate::value::Value;
use crate::{QmlObject, WriteKind, qml_impl};

/// The keys of one map, in the order they were added.
#[derive(Debug, Default)]
struct MapState {
    keys: RefCell<Vec<Atom>>,
    frozen: Cell<bool>,
}

impl MapState {
    fn has(&self, key: &str) -> bool {
        self.keys.borrow().iter().any(|k| k == key)
    }
}

/// The native object QML sees. In no module: like Qt's, it is made by
/// the application, never in QML.
#[derive(Debug, QmlObject)]
#[qml(name = "PropertyMap", extends = QtObject)]
pub(crate) struct PropertyMapObject {
    #[qml(base)]
    pub object: QtObject,
    #[qml(skip)]
    state: Rc<MapState>,
}

impl Default for PropertyMapObject {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            state: Rc::new(MapState::default()),
        }
    }
}

#[qml_impl]
impl PropertyMapObject {
    /// Emitted when QML writes a key, with the key and its new value.
    /// Never for the application's own writes.
    #[qml(signal)]
    fn value_changed(key: Atom, value: Value);

    /// The keys, in the order they were added. A cleared key is listed.
    #[qml(invokable)]
    fn keys(&self) -> Value {
        Value::list(
            self.state
                .keys
                .borrow()
                .iter()
                .map(|k| Value::String(k.clone()))
                .collect(),
        )
    }
}

thread_local! {
    /// Map types by key list. A type derives from the one for the list
    /// without its last key, so a map gaining a key takes the wider
    /// type in place (as `model.rs` row types do).
    static MAP_METAS: RefCell<HashMap<Vec<Atom>, MetaRef>> = RefCell::new(HashMap::new());
}

fn meta_for(keys: &[Atom]) -> MetaRef {
    if keys.is_empty() {
        return MetaRef::Static(<PropertyMapObject as QmlType>::static_meta());
    }
    if let Some(m) = MAP_METAS.with(|c| c.borrow().get(keys).cloned()) {
        return m;
    }
    let (last, rest) = keys.split_last().expect("keys is not empty");
    let base = meta_for(rest);
    let b = MetaBuilder::new("PropertyMap").base(base.clone());
    // A key named like a property the object already has (`objectName`)
    // adds nothing: the existing property is the one QML reads.
    let meta = if base.property_index(last).is_some() {
        b.build_shared()
    } else {
        b.property(last, TypeTag::Var, PropertyFlags::empty(), true)
            .build_shared()
    };
    MAP_METAS.with(|c| {
        c.borrow_mut().insert(keys.to_vec(), meta.clone());
    });
    meta
}

/// Handles a write from QML to a key: the value is stored, then
/// `valueChanged` is emitted — not for a value the key already has,
/// which changes nothing in Qt (measured on Qt 6.11.2). False: not a
/// key, let the write go its usual way.
fn qml_write(
    engine: &Engine,
    state: &Rc<MapState>,
    obj: ObjectId,
    prop: PropIndex,
    v: &Value,
) -> bool {
    let Some(name) = engine
        .meta_of(obj)
        .and_then(|m| m.property(prop).map(|p| p.name.clone()))
    else {
        return false;
    };
    if !state.has(&name) {
        return false;
    }
    let old = engine.untrack(|| engine.read(obj, prop));
    let _ = engine.write_internal(obj, prop, v.clone(), WriteKind::Imperative);
    if !old.change_equals(v)
        && let Some(s) = <PropertyMapObject as QmlType>::static_meta().signal_index("valueChanged")
    {
        let _ = engine.emit(obj, s, &[Value::String(name), v.clone()]);
    }
    true
}

/// A property map the application gives to QML: see the module docs.
/// Cloning gives another handle to the same map.
#[derive(Clone)]
pub struct PropertyMap {
    id: ObjectId,
    state: Rc<MapState>,
}

impl std::fmt::Debug for PropertyMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PropertyMap")
            .field("object", &self.id)
            .field("size", &self.size())
            .finish()
    }
}

impl PropertyMap {
    /// A new, empty map, owned by `owner` when there is one.
    pub fn new(engine: &Engine, owner: Option<ObjectId>) -> Result<PropertyMap, EngineError> {
        let state = Rc::new(MapState::default());
        let id = engine.create_native_incomplete(
            PropertyMapObject {
                object: QtObject::default(),
                state: state.clone(),
            },
            owner,
        )?;
        let hook_state = state.clone();
        engine.set_write_hook(
            id,
            Some(Rc::new(move |e: &Engine, prop: PropIndex, v: &Value| {
                qml_write(e, &hook_state, id, prop, v)
            })),
        );
        Ok(PropertyMap { id, state })
    }

    /// The map of the object `obj`, when it is one: how application code
    /// takes hold of a map it was handed back from QML.
    pub fn from_object(engine: &Engine, obj: ObjectId) -> Option<PropertyMap> {
        let state = engine
            .with::<PropertyMapObject, _>(obj, |m| m.state.clone())
            .ok()?;
        Some(PropertyMap { id: obj, state })
    }

    /// The object to hand to QML, in a context or a property.
    pub fn object(&self) -> ObjectId {
        self.id
    }

    /// Sets `key` to `value`, adding the key when the map has not got
    /// it. On a frozen map a new key is refused silently, as in Qt.
    /// Never emits `valueChanged`.
    pub fn insert(&self, engine: &Engine, key: &str, value: Value) {
        if !self.ensure_key(engine, key) {
            return;
        }
        if let Ok(p) = engine.property_index(self.id, key) {
            let _ = engine.write_internal(self.id, p, value, WriteKind::Imperative);
        }
    }

    /// [`PropertyMap::insert`] for many keys at once (Qt's whole-map
    /// `insert`): keys and values alike are taken in order.
    pub fn insert_many(&self, engine: &Engine, entries: impl IntoIterator<Item = (String, Value)>) {
        engine.batch(|| {
            for (key, value) in entries {
                self.insert(engine, &key, value);
            }
        });
    }

    /// Takes `key`'s value away: it reads `undefined` in QML and stays
    /// in [`PropertyMap::keys`]. A key the map has not got is added, as
    /// in Qt (unless the map is frozen). Never emits `valueChanged`.
    pub fn clear(&self, engine: &Engine, key: &str) {
        self.insert(engine, key, Value::Undefined);
    }

    /// Whether the map has `key`, cleared or not.
    pub fn contains(&self, key: &str) -> bool {
        self.state.has(key)
    }

    /// The value of `key`, `undefined` when it was cleared or the map
    /// has not got it.
    pub fn value(&self, engine: &Engine, key: &str) -> Value {
        engine.untrack(|| engine.read_by_name(self.id, key))
    }

    /// The keys, in the order they were added.
    pub fn keys(&self) -> Vec<Atom> {
        self.state.keys.borrow().clone()
    }

    /// How many keys the map has.
    pub fn size(&self) -> usize {
        self.state.keys.borrow().len()
    }

    /// Whether the map has no keys.
    pub fn is_empty(&self) -> bool {
        self.state.keys.borrow().is_empty()
    }

    /// No key may be added after this; the ones there are still change.
    pub fn freeze(&self) {
        self.state.frozen.set(true);
    }

    /// What to call when QML writes a key: the key and its new value.
    pub fn on_value_changed(
        &self,
        engine: &Engine,
        handler: impl Fn(&str, &Value) + 'static,
    ) -> Result<ConnectionId, EngineError> {
        engine.connect_by_name(self.id, "valueChanged", move |_, args| {
            let key = args.first().cloned().unwrap_or_default().to_js_string();
            let value = args.get(1).cloned().unwrap_or_default();
            handler(&key, &value);
        })
    }

    /// Adds `key` when it is new, widening the object's type to hold it.
    /// False when the frozen map refuses it.
    fn ensure_key(&self, engine: &Engine, key: &str) -> bool {
        if self.state.has(key) {
            return true;
        }
        if self.state.frozen.get() {
            return false;
        }
        self.state.keys.borrow_mut().push(Atom::intern(key));
        let keys = self.state.keys.borrow().clone();
        engine.extend_meta(self.id, meta_for(&keys)).is_ok()
    }
}
