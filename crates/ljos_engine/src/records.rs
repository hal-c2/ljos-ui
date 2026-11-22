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
//! keeps nothing alive but the script object; it is swept whenever it has
//! gained an eighth of its live entries, and at least a few, since the last
//! sweep. A replaced record's script object is so let go within a few newer
//! records in a small table and within an eighth of the live ones in a
//! large one, and filling a table stays linear: a sweep of N live entries
//! is paid for by the N/8 inserts before it. A value only one holder has
//! (a temporary being converted) is not kept: no later read can find it.
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

/// The fewest records the table may gain between sweeps. A sweep is the
/// only place the script object of a record nobody holds is let go, so
/// this bounds how many replaced generations pile up in the script heap
/// under steady republishing. Waiting for the table to double instead let
/// a large live table hold as many dead ones as live: a shell republishing
/// a state tree on every frame of news grew its heap to the cap before the
/// first sweep could free a generation.
const SWEEP_GROWTH: usize = 64;

/// A sweep waits for the table to gain this fraction of its live entries
/// when that is more than [`SWEEP_GROWTH`]. A fixed gap made filling a
/// table of N records cost N²/128 checks (a 50k-row `ListModel` about 20M
/// on the UI thread); a gap proportional to the table keeps each insert's
/// share of the sweeps constant.
const SWEEP_SHARE: usize = 8;

/// When the table holding `live` entries is next swept.
fn next_sweep(live: usize) -> usize {
    live + (live / SWEEP_SHARE).max(SWEEP_GROWTH)
}

pub(crate) struct Records {
    /// No script object for a record that is copied on each read.
    by_address: RefCell<HashMap<usize, (Held, Option<JsRef>)>>,
    sweep_at: Cell<usize>,
    /// Entries every sweep so far has looked at.
    #[cfg(test)]
    checked: Cell<usize>,
}

impl Default for Records {
    fn default() -> Self {
        Records {
            by_address: RefCell::default(),
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
            (Arc::downgrade(fields), JsRef::object(make()))
        })
    }

    /// Every read of the record `fields` gives a new script object, for
    /// as long as it is held.
    pub(crate) fn copy_each_read(&self, fields: &Arc<Vec<(Atom, Value)>>) {
        let at = Arc::as_ptr(fields) as usize;
        self.keep(at, Arc::downgrade(fields), None);
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
            return r.clone().map(JsValue::Handle);
        }
        if holders < 2 {
            return None;
        }
        let (held, r) = make();
        self.keep(at, held, Some(r.clone()));
        Some(JsValue::Handle(r))
    }

    fn keep(&self, at: usize, held: Held, r: Option<JsRef>) {
        let mut table = self.by_address.borrow_mut();
        table.insert(at, (held, r));
        if table.len() >= self.sweep_at.get() {
            #[cfg(test)]
            self.checked.set(self.checked.get() + table.len());
            table.retain(|_, (held, _)| held.strong_count() > 0);
            self.sweep_at.set(next_sweep(table.len()));
        }
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

    /// Under steady replacing, the dead entries in the table stay under an
    /// eighth of the live ones (or a few in a small table) however long
    /// the churn goes on.
    #[test]
    fn replaced_records_stay_a_bounded_share() {
        let records = Records::default();
        let live: Vec<_> = (0..20_000).map(record).collect();
        for r in &live {
            records.copy_each_read(r);
        }
        let mut current = record(0);
        records.copy_each_read(&current);
        let mut most_dead = 0;
        for n in 1..100_000 {
            current = record(n);
            records.copy_each_read(&current);
            // Every entry but the live ones and the current record is dead.
            let len = records.by_address.borrow().len();
            most_dead = most_dead.max(len - live.len() - 1);
        }
        let bound = (live.len() + 1) / SWEEP_SHARE + 1;
        assert!(
            most_dead <= bound,
            "{most_dead} replaced records were held at once beside {} live ones \
             (bound {bound})",
            live.len() + 1
        );
        drop(current);
    }
}
