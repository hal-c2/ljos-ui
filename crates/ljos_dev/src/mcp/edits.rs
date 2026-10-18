//! What devtools changed, so that it can be told apart from what the
//! application did and taken back.
//!
//! An edit is an assignment, as a script makes one, which replaces a
//! binding for good; or, with `keep_binding`, an override layer on top of
//! the binding, as `Binding {}` makes one, which leaves the binding where
//! it was. A binding devtools makes from an expression is either kind too.
//! Either way the value from before the first edit is kept: a
//! revert removes the layer and the binding under it comes back, or puts
//! that value back.

use std::collections::HashMap;

use ljos_engine::{ObjectId, OverrideId, Value as QmlValue};

/// One property devtools changed.
#[derive(Debug)]
pub(crate) struct Edit {
    /// What it held before devtools first changed it.
    pub(crate) before: QmlValue,
    /// The layer that covers its binding, for an edit that kept it.
    pub(crate) cover: Option<OverrideId>,
    /// The expression devtools bound it to, while that binding is what the
    /// edit put there.
    pub(crate) expression: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct Edits {
    by_property: HashMap<(ObjectId, String), Edit>,
}

impl Edits {
    pub(crate) fn contains(&self, obj: ObjectId, property: &str) -> bool {
        self.by_property.contains_key(&(obj, property.to_owned()))
    }

    /// Whether the edit of the property left its binding under it.
    pub(crate) fn covers(&self, obj: ObjectId, property: &str) -> bool {
        self.by_property
            .get(&(obj, property.to_owned()))
            .is_some_and(|e| e.cover.is_some())
    }

    /// The expression devtools bound the property to.
    pub(crate) fn expression(&self, obj: ObjectId, property: &str) -> Option<&str> {
        self.by_property
            .get(&(obj, property.to_owned()))
            .and_then(|e| e.expression.as_deref())
    }

    pub(crate) fn take(&mut self, obj: ObjectId, property: &str) -> Option<Edit> {
        self.by_property.remove(&(obj, property.to_owned()))
    }

    pub(crate) fn put(&mut self, obj: ObjectId, property: &str, edit: Edit) {
        self.by_property.insert((obj, property.to_owned()), edit);
    }
}
