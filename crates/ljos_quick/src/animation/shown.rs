//! What an Animator shows of a property while it runs. Measured on Qt: the
//! property itself keeps its old value until the animator ends, while the
//! picture moves; so the values on their way are kept here, apart from the
//! properties, and only the scene reads them.

use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{Engine, ObjectId, PropIndex};
use rustc_hash::FxHashMap;

type Watcher = Rc<dyn Fn(&Engine, ObjectId, PropIndex) -> bool>;

#[derive(Default)]
pub(crate) struct Shown {
    values: RefCell<FxHashMap<(ObjectId, PropIndex), f64>>,
    /// Told of each change; one that returns false is dropped.
    watchers: RefCell<Vec<Watcher>>,
}

impl Shown {
    pub(crate) fn get(&self, obj: ObjectId, prop: PropIndex) -> Option<f64> {
        let values = self.values.borrow();
        if values.is_empty() {
            return None;
        }
        values.get(&(obj, prop)).copied()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.values.borrow().is_empty()
    }
}

pub(crate) fn shown(engine: &Engine) -> Rc<Shown> {
    engine.extension::<Shown>()
}

/// Shows `value` for the property, or with `None` the property again.
pub(crate) fn show(engine: &Engine, obj: ObjectId, prop: PropIndex, value: Option<f64>) {
    let s = shown(engine);
    let changed = {
        let mut values = s.values.borrow_mut();
        match value {
            Some(v) => values.insert((obj, prop), v) != Some(v),
            None => values.remove(&(obj, prop)).is_some(),
        }
    };
    if !changed {
        return;
    }
    // Called unborrowed: a watcher reads the values.
    let watchers: Vec<Watcher> = s.watchers.borrow().clone();
    let gone: Vec<Watcher> = watchers
        .into_iter()
        .filter(|w| !w(engine, obj, prop))
        .collect();
    if !gone.is_empty() {
        s.watchers
            .borrow_mut()
            .retain(|w| !gone.iter().any(|g| Rc::ptr_eq(g, w)));
    }
}

/// Calls `f` after each change of what is shown; `f` returns false once
/// it no longer wants to hear.
pub(crate) fn watch(engine: &Engine, f: impl Fn(&Engine, ObjectId, PropIndex) -> bool + 'static) {
    shown(engine).watchers.borrow_mut().push(Rc::new(f));
}
