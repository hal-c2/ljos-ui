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
use crate::meta::PropertyAccess;
use crate::object::{ObjectHeader, find_native_mut};
use crate::types::{ConvertError, QmlValue, TypeTag};
use crate::value::Value;

/// The field type of a native list property: the objects, in order.
///
/// It holds the list in the form a read of the property gives
/// ([`Value::List`]), so that reading it is not a copy, and so that the
/// engine can append to it without one.
#[derive(Clone)]
pub struct QmlList(Arc<Vec<Value>>);

impl Default for QmlList {
    /// One empty list shared by all: an item has half a dozen list
    /// properties and most stay empty. The first write makes its own.
    fn default() -> Self {
        thread_local! {
            static EMPTY: Arc<Vec<Value>> = Arc::new(Vec::new());
        }
        QmlList(EMPTY.with(Arc::clone))
    }
}

impl QmlList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn iter(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.0.iter().filter_map(Value::as_object)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The objects, copied out.
    pub fn objects(&self) -> Vec<ObjectId> {
        self.iter().collect()
    }
}

impl From<Vec<ObjectId>> for QmlList {
    fn from(objects: Vec<ObjectId>) -> Self {
        QmlList(Arc::new(objects.into_iter().map(Value::Object).collect()))
    }
}

impl FromIterator<ObjectId> for QmlList {
    fn from_iter<I: IntoIterator<Item = ObjectId>>(objects: I) -> Self {
        QmlList(Arc::new(objects.into_iter().map(Value::Object).collect()))
    }
}

// Two lists are equal when they hold the same objects, whichever
// allocation each sits in (a `Value::List` is equal only to itself).
impl PartialEq for QmlList {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.iter().eq(other.iter())
    }
}

impl Eq for QmlList {}

impl std::fmt::Debug for QmlList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl QmlValue for QmlList {
    fn type_tag() -> TypeTag {
        TypeTag::List(None)
    }

    fn to_value(&self) -> Value {
        Value::List(self.0.clone())
    }

    fn from_value(v: &Value) -> Result<Self, ConvertError> {
        match Self::type_tag().coerce(v)? {
            // A list of objects keeps only the objects, as Qt drops what a
            // QQmlListProperty cannot hold. The usual list holds nothing
            // else and is shared as it is.
            Value::List(l) if l.iter().all(|v| matches!(v, Value::Object(_))) => Ok(QmlList(l)),
            Value::List(l) => Ok(l.iter().filter_map(Value::as_object).collect()),
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
            let h = store
                .objects
                .get(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
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

    /// Puts `item` right before `anchor` in the list property `prop` of
    /// `obj`, through the property's insert hook if it has one, or at the
    /// end when `anchor` is not in the list. `false`, and nothing done,
    /// when the property's appends go through a hook that has no insert
    /// hook beside it, or it is not a list.
    pub fn list_insert(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        item: ObjectId,
        anchor: ObjectId,
    ) -> Result<bool, EngineError> {
        let (obj, prop) = self.redirect(obj, prop);
        let hooks = {
            let store = self.inner.store.borrow();
            let h = store
                .objects
                .get(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            h.meta
                .property(prop)
                .filter(|p| p.is_list())
                .map(|p| (p.list_append, p.list_insert))
        };
        match hooks {
            Some((_, Some(f))) => f(&Cx::new(self, obj), item, anchor).map(|()| true),
            Some((None, None)) => {
                if !self.list_insert_before(obj, prop, item, anchor)? {
                    self.list_push(obj, prop, item)?;
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Shortens the list property `prop` of `obj` to `len` entries, as a
    /// script does by setting its length: the entries after it are taken
    /// out from the last, each through the property's remove hook when it
    /// has one (an item taken out of `children` has no parent then).
    pub fn list_truncate(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        len: usize,
    ) -> Result<(), EngineError> {
        let (obj, prop) = self.redirect(obj, prop);
        let hook = {
            let store = self.inner.store.borrow();
            let h = store
                .objects
                .get(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            h.meta.property(prop).and_then(|p| p.list_remove)
        };
        let taken: Vec<Value> = match self.untrack(|| self.read(obj, prop)) {
            Value::List(l) if l.len() > len => l[len..].iter().rev().cloned().collect(),
            _ => return Ok(()),
        };
        self.batch(|| {
            match hook {
                Some(f) => {
                    for item in taken.iter().filter_map(Value::as_object) {
                        f(&Cx::new(self, obj), item)?;
                    }
                }
                None => self.list_edit(obj, prop, &|items| items.truncate(len))?,
            }
            Ok(())
        })
    }

    /// Whether [`Engine::list_insert`] can put an item of the list property
    /// `prop` of `obj` before another.
    pub(crate) fn list_takes_insert(&self, obj: ObjectId, prop: PropIndex) -> bool {
        let (obj, prop) = self.redirect(obj, prop);
        let store = self.inner.store.borrow();
        store
            .objects
            .get(obj)
            .and_then(|h| h.meta.property(prop))
            .is_some_and(|p| p.is_list() && (p.list_insert.is_some() || p.list_append.is_none()))
    }

    /// Appends without the hook: what a hook calls to store the item in
    /// the list it routes to. The list is changed where it is, so the cost
    /// does not grow with its length.
    pub fn list_push(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        item: ObjectId,
    ) -> Result<(), EngineError> {
        self.list_edit(obj, prop, &|items| items.push(Value::Object(item)))
    }

    /// Puts `item` right before `anchor`, which is looked for from the end
    /// of the list. `false`, and no change, when `anchor` is not in it.
    pub fn list_insert_before(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        item: ObjectId,
        anchor: ObjectId,
    ) -> Result<bool, EngineError> {
        // The clone of the list is gone before the edit, which would
        // otherwise have to copy what the clone still shares.
        let at = match self.untrack(|| self.read(obj, prop)) {
            Value::List(l) => l.iter().rposition(|v| v.as_object() == Some(anchor)),
            _ => None,
        };
        let Some(at) = at else {
            return Ok(false);
        };
        self.list_edit(obj, prop, &|items| {
            items.insert(at.min(items.len()), Value::Object(item));
        })?;
        Ok(true)
    }

    /// Takes `item` out, looking for it from the end of the list. `false`,
    /// and no change, when it is not in it.
    pub fn list_remove(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        item: ObjectId,
    ) -> Result<bool, EngineError> {
        let at = match self.untrack(|| self.read(obj, prop)) {
            Value::List(l) => l.iter().rposition(|v| v.as_object() == Some(item)),
            _ => None,
        };
        let Some(at) = at else {
            return Ok(false);
        };
        self.list_edit(obj, prop, &|items| {
            if items.get(at).and_then(Value::as_object) == Some(item) {
                items.remove(at);
            }
        })?;
        Ok(true)
    }

    /// Changes a list property through `edit`, which must change it.
    ///
    /// The list has two holders at most, the object's field and the
    /// property's node in the graph, and they share one allocation. The
    /// edit is made in that allocation whenever nothing else (a reader
    /// that kept the value) shares it; only then is it copied.
    pub(crate) fn list_edit(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        edit: &dyn Fn(&mut Vec<Value>),
    ) -> Result<(), EngineError> {
        let (obj, prop) = self.redirect(obj, prop);
        let apply = |v: &mut Value| match v {
            Value::List(l) => edit(Arc::make_mut(l)),
            other => {
                let mut items = Vec::new();
                edit(&mut items);
                *other = Value::list(items);
            }
        };
        let node = {
            let mut store = self.inner.store.borrow_mut();
            let h = store
                .objects
                .get_mut(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            let node = match h.node(prop) {
                None if h.read_without_node(prop) => {
                    // A reader follows it: the edit goes through a node.
                    drop(store);
                    self.ensure_node(obj, prop)?;
                    return self.list_edit(obj, prop, edit);
                }
                n => n,
            };
            let Some(slot) = list_slot(h, prop) else {
                // Not a list kept where the engine can reach it.
                drop(store);
                return self.list_edit_whole(obj, prop, edit);
            };
            match node {
                // Nobody observes it: the field is the only copy.
                None => {
                    slot.edit(&apply);
                    h.mark_assigned(prop);
                    let change = crate::engine::change_hook(h, prop);
                    drop(store);
                    self.tell_change(obj, prop, change);
                    return Ok(());
                }
                // The node's is the one that is edited; the field lets go
                // of its share first, so that there is one holder.
                Some(n) => {
                    slot.put(Value::List(Arc::default()));
                    n
                }
            }
        };
        let g = &self.inner.graph;
        // Batched so effects run after the field has the list again.
        let done = g.batch(|| -> Result<bool, EngineError> {
            let done = g.modify(node, |v| apply(v))?;
            if done && let Some(cur) = g.peek(node) {
                let mut store = self.inner.store.borrow_mut();
                if let Some(slot) = store.objects.get_mut(obj).and_then(|h| list_slot(h, prop)) {
                    slot.put(cur);
                }
            }
            Ok(done)
        })?;
        if done {
            return Ok(());
        }
        // Bound, overridden or dirty: the usual write decides what the new
        // list means for that, and fills the field again.
        self.list_edit_whole(obj, prop, edit)
    }

    /// [`Engine::list_edit`] the long way: a new list, written.
    fn list_edit_whole(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        edit: &dyn Fn(&mut Vec<Value>),
    ) -> Result<(), EngineError> {
        let mut items = self.list_items(obj, prop);
        edit(&mut items);
        self.write_internal(obj, prop, Value::list(items), WriteKind::Initial)
    }

    /// Appends to the type's default property.
    pub(crate) fn list_append_default(
        &self,
        obj: ObjectId,
        item: ObjectId,
    ) -> Result<(), EngineError> {
        let prop = self.default_property(obj)?;
        self.list_append(obj, prop, item)
    }

    /// The default property's index.
    pub fn default_property(&self, obj: ObjectId) -> Result<PropIndex, EngineError> {
        let store = self.inner.store.borrow();
        let h = store
            .objects
            .get(obj)
            .ok_or_else(|| EngineError::dead(obj))?;
        if let Some(p) = h.meta.default_property_index() {
            return Ok(p);
        }
        Err(EngineError::UnknownProperty {
            ty: h.meta.name.to_string(),
            // The name a type gives that it does not have, else none.
            name: h
                .meta
                .default_property()
                .map_or_else(|| "default property".into(), |n| n.to_string()),
        })
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

/// Where an object keeps a list property's items.
enum ListSlot<'a> {
    /// A native field.
    Field(&'a mut QmlList),
    /// A property declared in QML.
    Dynamic(&'a mut Value),
}

impl ListSlot<'_> {
    fn edit(self, apply: &dyn Fn(&mut Value)) {
        match self {
            ListSlot::Field(list) => {
                let mut v = Value::List(std::mem::take(list).0);
                apply(&mut v);
                if let Value::List(l) = v {
                    list.0 = l;
                }
            }
            ListSlot::Dynamic(v) => apply(v),
        }
    }

    /// Stores a list the engine made: objects only, which is why a field
    /// takes it as it is.
    fn put(self, v: Value) {
        match self {
            ListSlot::Field(list) => {
                if let Value::List(l) = v {
                    list.0 = l;
                }
            }
            ListSlot::Dynamic(slot) => *slot = v,
        }
    }
}

/// The entries the list property `prop` holds, where they are held.
pub(crate) fn held_entries(h: &mut ObjectHeader, prop: PropIndex) -> Option<&[Value]> {
    match list_slot(h, prop)? {
        ListSlot::Field(list) => Some(list.0.as_slice()),
        ListSlot::Dynamic(Value::List(items)) => Some(items.as_slice()),
        ListSlot::Dynamic(_) => None,
    }
}

fn list_slot(h: &mut ObjectHeader, prop: PropIndex) -> Option<ListSlot<'_>> {
    let p = h.meta.property(prop)?;
    if !p.is_list() {
        return None;
    }
    match (p.access, p.list_field) {
        (PropertyAccess::Field { owner, .. }, Some(field)) => {
            find_native_mut(&mut *h.native, owner).map(|o| ListSlot::Field(field(o)))
        }
        (PropertyAccess::Dynamic { slot }, _) => {
            h.dynamic.get_mut(slot as usize).map(ListSlot::Dynamic)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An item's half-dozen list properties start empty without a list
    /// made for each.
    #[test]
    fn empty_lists_share_one_allocation_until_written() {
        let (Value::List(a), Value::List(b)) =
            (QmlList::new().to_value(), QmlList::default().to_value())
        else {
            panic!("a list reads as a list");
        };
        assert!(Arc::ptr_eq(&a, &b));
        let mut c = QmlList::new();
        ListSlot::Field(&mut c).edit(&|v| {
            if let Value::List(l) = v {
                Arc::make_mut(l).push(Value::Int(1));
            }
        });
        assert_eq!(c.len(), 1);
        assert!(
            QmlList::new().is_empty(),
            "the shared empty list stays empty"
        );
    }
}
