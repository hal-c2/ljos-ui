//! A record Rust handed QML (a [`Value::Map`] stored in a property, a
//! `PropertyMap` entry or a signal argument) is one script object for as
//! long as that value is held: every read gives the same object, so
//! `h.state === h.state` holds and a field read does not copy the record.
//! Setting the property again stores a new value, and so a new object. The
//! lists inside a record are part of that object.
//!
//! A list on its own ([`Value::List`]) is still a new array on each read:
//! a list property's value is a sequence, which Qt copies when a script
//! hands it on (`features/language/list-properties-held.feature`).
//!
//! The table is keyed by the value's allocation and holds it weakly, so it
//! keeps nothing alive but the script object; it is swept when it has
//! doubled since the last sweep. A value only one holder has (a temporary
//! being converted) is not kept: no later read can find it.
//!
//! What a script writes into the object is seen by the next script read of
//! the same value but not by Rust, which still has the value it stored.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::{Arc, Weak};

use ljos_js::{JsRef, JsValue};

use crate::atom::Atom;
use crate::value::Value;

type Held = Weak<Vec<(Atom, Value)>>;

pub(crate) struct Records {
    by_address: RefCell<HashMap<usize, (Held, JsRef)>>,
    sweep_at: Cell<usize>,
}

impl Default for Records {
    fn default() -> Self {
        Records {
            by_address: RefCell::default(),
            sweep_at: Cell::new(64),
        }
    }
}

impl Records {
    /// The script object for the record `fields`, made by `make` the
    /// first time; `None` for a record nobody else holds.
    pub(crate) fn map(
        &self,
        fields: &Arc<Vec<(Atom, Value)>>,
        make: impl FnOnce() -> Vec<(String, JsValue)>,
    ) -> Option<JsValue> {
        let at = Arc::as_ptr(fields) as usize;
        self.find(at, Arc::strong_count(fields), || {
            (Arc::downgrade(fields), JsRef::object(make()))
        })
    }

    fn find(
        &self,
        at: usize,
        holders: usize,
        make: impl FnOnce() -> (Held, JsRef),
    ) -> Option<JsValue> {
        // The allocation is the key: a weak reference in the table keeps it
        // from being reused by another value while the entry is there.
        if let Some((held, r)) = self.by_address.borrow().get(&at)
            && held.strong_count() > 0
        {
            return Some(JsValue::Handle(r.clone()));
        }
        if holders < 2 {
            return None;
        }
        let (held, r) = make();
        let mut table = self.by_address.borrow_mut();
        table.insert(at, (held, r.clone()));
        if table.len() >= self.sweep_at.get() {
            table.retain(|_, (held, _)| held.strong_count() > 0);
            self.sweep_at.set((table.len() * 2).max(64));
        }
        Some(JsValue::Handle(r))
    }
}
