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
//! keeps nothing alive but the script object. It is swept whenever the
//! script bytes it keeps have grown by an eighth of the live ones, and at
//! least by [`SWEEP_FLOOR`]: replaced records so hold no more than that of
//! the script heap, whatever their size and however long each was held,
//! and filling the table stays linear. Counting entries instead let a few
//! dead trees, each one entry, outweigh thousands of live small records.
//! A value only one holder has (a temporary being converted) is not kept:
//! no later read can find it.
//!
//! What a script writes into the object is seen by the next script read of
//! the same value but not by Rust, which still has the value it stored.
//!
//! A record a `ListModel` row keeps is the other way: Qt gives a new copy of
//! it on each read, and what a script writes into one reaches nothing
//! ([`Records::copy_each_read`]).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::{Arc, Weak};

use ljos_js::{JsRef, JsValue};

use crate::atom::Atom;
use crate::value::Value;

type Held = Weak<Vec<(Atom, Value)>>;

/// The fewest bytes the table may gain between sweeps. A sweep is the only
/// place the script object of a record nobody holds is let go, so this and
/// [`SWEEP_SHARE`] bound what replaced generations hold of the script heap
/// under steady republishing.
const SWEEP_FLOOR: usize = 256 * 1024;

/// A sweep waits for the table to gain this fraction of its live bytes when
/// that is more than [`SWEEP_FLOOR`]. A fixed gap made filling a table of N
/// records cost N²/128 checks (a 50k-row `ListModel` about 20M on the UI
/// thread).
const SWEEP_SHARE: usize = 8;

/// What an entry or a script object counts for, before what is in it.
const OBJECT_BYTES: usize = 64;

/// What a value in a script object counts for, before what is in it.
const VALUE_BYTES: usize = 16;

/// The bytes at which a table keeping `live` bytes is next swept.
fn next_sweep(live: usize) -> usize {
    live + (live / SWEEP_SHARE).max(SWEEP_FLOOR)
}

/// About what an object of `fields` takes of the script heap. A record
/// inside it that is kept itself is counted by its own entry.
fn object_bytes(fields: &[(String, JsValue)]) -> usize {
    OBJECT_BYTES
        + fields
            .iter()
            .map(|(name, value)| name.len() + value_bytes(value))
            .sum::<usize>()
}

fn value_bytes(value: &JsValue) -> usize {
    VALUE_BYTES
        + match value {
            JsValue::String(text) => text.len(),
            JsValue::Array(items) => OBJECT_BYTES + items.iter().map(value_bytes).sum::<usize>(),
            JsValue::Map(fields) => object_bytes(fields),
            _ => 0,
        }
}

struct Entry {
    held: Held,
    /// None for a record that is copied on each read.
    object: Option<JsRef>,
    bytes: usize,
}

pub(crate) struct Records {
    by_address: RefCell<HashMap<usize, Entry>>,
    /// The sum of the entries' bytes.
    bytes: Cell<usize>,
    sweep_at: Cell<usize>,
    /// Entries every sweep so far has looked at.
    #[cfg(test)]
    checked: Cell<usize>,
}

impl Default for Records {
    fn default() -> Self {
        Records {
            by_address: RefCell::default(),
            bytes: Cell::new(0),
            sweep_at: Cell::new(next_sweep(0)),
            #[cfg(test)]
            checked: Cell::new(0),
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
            let made = make();
            Entry {
                held: Arc::downgrade(fields),
                bytes: object_bytes(&made),
                object: Some(JsRef::object(made)),
            }
        })
    }

    /// Every read of the record `fields` gives a new script object, for
    /// as long as it is held.
    pub(crate) fn copy_each_read(&self, fields: &Arc<Vec<(Atom, Value)>>) {
        let at = Arc::as_ptr(fields) as usize;
        let entry = Entry {
            held: Arc::downgrade(fields),
            object: None,
            bytes: OBJECT_BYTES,
        };
        self.keep(at, entry);
    }

    fn find(&self, at: usize, holders: usize, make: impl FnOnce() -> Entry) -> Option<JsValue> {
        // The allocation is the key: a weak reference in the table keeps it
        // from being reused by another value while the entry is there.
        if let Some(entry) = self.by_address.borrow().get(&at)
            && entry.held.strong_count() > 0
        {
            return entry.object.clone().map(JsValue::Handle);
        }
        if holders < 2 {
            return None;
        }
        let entry = make();
        let object = entry.object.clone();
        self.keep(at, entry);
        object.map(JsValue::Handle)
    }

    fn keep(&self, at: usize, entry: Entry) {
        let mut table = self.by_address.borrow_mut();
        let mut bytes = self.bytes.get() + entry.bytes;
        if let Some(before) = table.insert(at, entry) {
            bytes -= before.bytes;
        }
        if bytes >= self.sweep_at.get() {
            #[cfg(test)]
            self.checked.set(self.checked.get() + table.len());
            bytes = 0;
            table.retain(|_, entry| {
                let live = entry.held.strong_count() > 0;
                if live {
                    bytes += entry.bytes;
                }
                live
            });
            self.sweep_at.set(next_sweep(bytes));
        }
        self.bytes.set(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(i: usize) -> Arc<Vec<(Atom, Value)>> {
        Arc::new(vec![(Atom::intern("i"), Value::Real(i as f64))])
    }

    /// Filling a table costs a constant number of sweep checks per record,
    /// whatever its size: a fixed gap between sweeps made it N²/128.
    #[test]
    fn filling_a_table_is_linear() {
        for n in [1_000, 10_000, 100_000] {
            let records = Records::default();
            let held: Vec<_> = (0..n).map(record).collect();
            for r in &held {
                records.copy_each_read(r);
            }
            assert_eq!(records.by_address.borrow().len(), n);
            let checked = records.checked.get();
            // The gaps grow by 9/8 a sweep, so the checks sum to about
            // 9 per record; 10 leaves room for the small first gaps.
            assert!(
                checked <= 10 * n,
                "filling {n} records checked {checked} entries in sweeps, \
                 {} per record",
                checked / n
            );
        }
    }

    /// What replaced records hold stays under an eighth of the live bytes,
    /// or the floor, plus the newest record: whether one key is republished
    /// or many in turn, so each record is held for a while before it dies.
    #[test]
    fn replaced_records_hold_a_bounded_share_of_the_bytes() {
        const TEXT: usize = 64 * 1024;
        for keys in [1, 64, 200] {
            let records = Records::default();
            let small: Vec<_> = (0..10_000).map(record).collect();
            for r in &small {
                records.copy_each_read(r);
            }
            let text = "x".repeat(TEXT);
            let publish = |n: usize| {
                let tree = record(n);
                // A second holder, as the property the script read it from is.
                let holder = tree.clone();
                let kept = records.map(&holder, || {
                    vec![("text".to_owned(), JsValue::String(text.clone()))]
                });
                assert!(kept.is_some(), "a held record is kept");
                tree
            };
            let mut current: Vec<_> = (0..keys).map(publish).collect();
            let one = object_bytes(&[("text".to_owned(), JsValue::String(text.clone()))]);
            let live = small.len() * OBJECT_BYTES + keys * one;
            let bound = (live / SWEEP_SHARE).max(SWEEP_FLOOR) + one;
            for n in 0..1_000 {
                current[n % keys] = publish(n);
                let held = records.bytes.get();
                assert!(
                    held <= live + bound,
                    "{} bytes of replaced records held beside {live} live ones \
                     with {keys} keys republished in turn (bound {bound})",
                    held - live
                );
            }
        }
    }
}
