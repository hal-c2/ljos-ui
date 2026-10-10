//! QML `list<T>` properties of objects (contract item 1).
//!
//! A native type declares one with `#[qml(list)]` on a [`QmlList`] field;
//! QML declares one with `property list<T> name`. Either reads as a
//! [`Value::List`] of objects. Appends go through [`Engine::list_append`],
//! which calls the property's append hook when it has one (`Item.data`
//! routes visual children and resources), so the object creator does not
//! need to know what a list means to its type.

use std::sync::Arc;

use ljos_reactive::WriteKind;

use crate::cx::Cx;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{ObjectId, PropIndex};
use crate::types::{ConvertError, QmlValue, TypeTag};
use crate::value::Value;

/// The field type of a native list property: the objects, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QmlList(pub Vec<ObjectId>);

impl QmlList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn iter(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.0.iter().copied()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl QmlValue for QmlList {
    fn type_tag() -> TypeTag {
        TypeTag::List(None)
    }

    fn to_value(&self) -> Value {
        Value::List(Arc::new(self.0.iter().map(|o| Value::Object(*o)).collect()))
    }

    fn from_value(v: &Value) -> Result<Self, ConvertError> {
        match Self::type_tag().coerce(v)? {
            // A list of objects keeps only the objects, as Qt drops what a
            // QQmlListProperty cannot hold.
            Value::List(l) => Ok(QmlList(l.iter().filter_map(Value::as_object).collect())),
            _ => unreachable!("coerce returns a list"),
        }
    }
}

impl Engine {
    /// Appends `item` to the list property `prop` of `obj`, through the
    /// property's append hook if it has one.
    pub fn list_append(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        item: ObjectId,
    ) -> Result<(), EngineError> {
        let (obj, prop) = self.redirect(obj, prop);
        let hook = {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
            let p = h
                .meta
                .property(prop)
                .ok_or_else(|| EngineError::UnknownProperty {
                    ty: h.meta.name.to_string(),
                    name: format!("#{}", prop.0),
                })?;
            if !p.is_list() && matches!(p.ty, TypeTag::Object(_)) {
                // A default property that holds one object
                // (`Repeater.delegate`): the object written last is it.
                drop(store);
                return self.write_internal(obj, prop, Value::Object(item), WriteKind::Initial);
            }
            p.list_append
        };
        match hook {
            Some(f) => f(&Cx::new(self, obj), item),
            None => self.list_push(obj, prop, item),
        }
    }

    /// Appends without the hook: what a hook calls to store the item in
    /// the list it routes to.
    pub fn list_push(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        item: ObjectId,
    ) -> Result<(), EngineError> {
        let mut items = self.list_items(obj, prop);
        items.push(Value::Object(item));
        self.write_internal(obj, prop, Value::list(items), WriteKind::Initial)
    }

    /// Appends to the type's default property.
    pub fn list_append_default(&self, obj: ObjectId, item: ObjectId) -> Result<(), EngineError> {
        let prop = self.default_property(obj)?;
        self.list_append(obj, prop, item)
    }

    /// The default property's index.
    pub fn default_property(&self, obj: ObjectId) -> Result<PropIndex, EngineError> {
        let name = {
            let store = self.inner.store.borrow();
            let h = store.objects.get(obj).ok_or(EngineError::DeadObject(obj))?;
            h.meta
                .default_property()
                .cloned()
                .ok_or_else(|| EngineError::UnknownProperty {
                    ty: h.meta.name.to_string(),
                    name: "default property".into(),
                })?
        };
        self.property_index(obj, &name)
    }

    /// Number of items (`list.length`). Tracked.
    pub fn list_count(&self, obj: ObjectId, prop: PropIndex) -> usize {
        match self.read(obj, prop) {
            Value::List(l) => l.len(),
            _ => 0,
        }
    }

    /// The item at `i`. Tracked.
    pub fn list_at(&self, obj: ObjectId, prop: PropIndex, i: usize) -> Option<ObjectId> {
        match self.read(obj, prop) {
            Value::List(l) => l.get(i).and_then(Value::as_object),
            _ => None,
        }
    }

    /// Empties the list. The items are not destroyed.
    pub fn list_clear(&self, obj: ObjectId, prop: PropIndex) -> Result<(), EngineError> {
        self.write_internal(obj, prop, Value::list(Vec::new()), WriteKind::Initial)
    }

    fn list_items(&self, obj: ObjectId, prop: PropIndex) -> Vec<Value> {
        match self.untrack(|| self.read(obj, prop)) {
            Value::List(l) => (*l).clone(),
            _ => Vec::new(),
        }
    }
}
