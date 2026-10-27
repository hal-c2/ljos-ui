//! What other crates register for a type and the types derived from it.

use std::cell::RefCell;

use ljos_engine::{Engine, MetaRef, ObjectId};

/// One value per registered type. A later registration wins, so a derived
/// type can be given a value of its own.
pub(crate) struct ByType<T>(RefCell<Vec<(MetaRef, T)>>);

impl<T> Default for ByType<T> {
    fn default() -> Self {
        Self(RefCell::new(Vec::new()))
    }
}

impl<T: Clone> ByType<T> {
    pub(crate) fn push(&self, meta: MetaRef, value: T) {
        self.0.borrow_mut().push((meta, value));
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.borrow().is_empty()
    }

    /// The value for `meta`, copied out: what it calls may register more.
    pub(crate) fn find(&self, meta: &MetaRef) -> Option<T> {
        self.0
            .borrow()
            .iter()
            .rev()
            .find(|(m, _)| meta.inherits(m))
            .map(|(_, v)| v.clone())
    }

    /// The value for `obj`'s type. Nothing registered asks nothing of the
    /// engine.
    pub(crate) fn of(&self, engine: &Engine, obj: ObjectId) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        self.find(&engine.meta_of(obj)?)
    }
}
