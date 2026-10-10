//! `FocusScope` and the types of other crates that are focus scopes.

use std::cell::RefCell;

use ljos_engine::{Engine, MetaRef, ObjectId, QmlObject, QmlType};

use crate::item::Item;

/// `FocusScope` (QtQuick): an item whose children share one focus item
/// of their own. The scope's `focus` then decides whether that item is
/// the one with active focus, so a component keeps its inner focus
/// arrangement private (doc.qt.io/qt-6/qml-qtquick-focusscope.html).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "FocusScope", extends = Item)]
pub struct FocusScope {
    #[qml(base)]
    pub item: Item,
}

#[derive(Default)]
struct Registry {
    types: RefCell<Vec<MetaRef>>,
}

/// Makes items of type `meta` (and of the types derived from it) focus
/// scopes. For a type of another crate that is a scope without deriving
/// from [`FocusScope`] (a popup's content, a view that keeps a current
/// item).
pub fn register_focus_scope(engine: &Engine, meta: MetaRef) {
    engine.extension::<Registry>().types.borrow_mut().push(meta);
}

/// Whether `obj` is a focus scope.
pub fn is_focus_scope(engine: &Engine, obj: ObjectId) -> bool {
    let Some(meta) = engine.meta_of(obj) else {
        return false;
    };
    if meta.inherits(FocusScope::static_meta()) {
        return true;
    }
    let reg = engine.extension::<Registry>();
    let types = reg.types.borrow();
    types.iter().any(|m| meta.inherits(m))
}
