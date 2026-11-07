//! A view's rows matched by key (docs/DIALECT.md §4.8, §4.15).
//!
//! A view given `key: "id"` and a list keeps the rows it shows when it is
//! given another list: a row whose key is in both keeps its delegate and
//! reads its new value, a row of a new key is inserted and a row whose key
//! is gone is removed. The view hears these as a model's changes, so it
//! makes and destroys only what changed, and plays its transitions on
//! those, where a new list without a key makes every delegate again.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use crate::model::{ItemModel, ModelChange, ModelListener, ModelListeners, RowKey};
use crate::{Atom, Engine, ObjectId, Value};

/// The rows of a list a keyed view shows. Each row is a value of the
/// list; its `modelData` is that value, and its roles are the value's
/// fields.
#[derive(Debug)]
pub struct KeyedRows {
    key: Atom,
    rows: RefCell<Vec<Row>>,
    /// The [`RowKey`] the next new row is given: a row keeps its own over
    /// moves, as a `ListModel`'s does.
    next: Cell<RowKey>,
    listeners: ModelListeners,
    /// Rows that share a key are told of once.
    warned: Cell<bool>,
    /// Where the view is declared, for what it says.
    at: String,
}

#[derive(Debug)]
struct Row {
    value: Value,
    key: String,
    id: RowKey,
}

impl KeyedRows {
    fn new(engine: &Engine, key: &str, rows: Vec<Value>, view: Option<ObjectId>) -> Self {
        let at = view.and_then(|o| engine.declared_at(o)).unwrap_or_default();
        let this = Self {
            key: Atom::from(key),
            rows: RefCell::new(Vec::new()),
            next: Cell::new(0),
            listeners: ModelListeners::default(),
            warned: Cell::new(false),
            at,
        };
        let keys: Vec<String> = rows.iter().map(|v| this.key_of(engine, v)).collect();
        this.warn_shared(engine, &keys);
        let made = this.rows_of(rows.into_iter().zip(keys));
        *this.rows.borrow_mut() = made;
        this
    }

    fn rows_of(&self, rows: impl Iterator<Item = (Value, String)>) -> Vec<Row> {
        rows.map(|(value, key)| {
            let id = self.next.get();
            self.next.set(id + 1);
            Row { value, key, id }
        })
        .collect()
    }

    /// The key of a row, as text: rows are matched by it.
    fn key_of(&self, engine: &Engine, row: &Value) -> String {
        let field = match row {
            Value::Object(o) => engine.read_by_name(*o, &self.key),
            Value::Js(_) | Value::Map(_) => engine
                .fields(row)
                .and_then(|f| f.into_iter().find(|(k, _)| *k == *self.key))
                .map(|(_, v)| v)
                .unwrap_or_default(),
            _ => Value::Undefined,
        };
        // Tagged, so that the key 1 is not the key "1".
        match field {
            Value::String(s) => format!("s{s}"),
            Value::Int(n) => format!("n{}", crate::number::number_to_string(f64::from(n))),
            Value::Real(n) => format!("n{}", crate::number::number_to_string(n)),
            Value::Bool(b) => format!("b{b}"),
            Value::Undefined => "u".to_string(),
            other => format!("o{other:?}"),
        }
    }

    fn warn_shared(&self, engine: &Engine, keys: &[String]) {
        if self.warned.get() {
            return;
        }
        let mut seen = std::collections::HashSet::new();
        if let Some(k) = keys.iter().find(|k| !seen.insert(*k)) {
            self.warned.set(true);
            engine.warn(format!(
                "{}rows share the key {} {}; they keep delegates in order: give each row a key of its own",
                self.at,
                self.key,
                k.get(1..).unwrap_or_default()
            ));
        }
    }

    /// Shows `new` in place of the rows shown: the rows of keys that stay
    /// keep their place in the view's eyes and are moved into the new
    /// order, then the rows of new keys are inserted, and kept rows whose
    /// value changed are told of. Rows that share a key are matched in
    /// order.
    pub fn update(&self, engine: &Engine, new: Vec<Value>) {
        let new_keys: Vec<String> = new.iter().map(|v| self.key_of(engine, v)).collect();
        self.warn_shared(engine, &new_keys);
        let mut wanted: HashMap<&str, VecDeque<usize>> = HashMap::new();
        for (i, k) in new_keys.iter().enumerate() {
            wanted.entry(k.as_str()).or_default().push_back(i);
        }
        let old_keys: Vec<String> = self.rows.borrow().iter().map(|r| r.key.clone()).collect();
        let target: Vec<Option<usize>> = old_keys
            .iter()
            .map(|k| wanted.get_mut(k.as_str()).and_then(VecDeque::pop_front))
            .collect();
        engine.batch(|| {
            // Removed from the end, so the indices before stay.
            for j in (0..old_keys.len()).rev() {
                if target[j].is_none() {
                    self.change(
                        engine,
                        ModelChange::RowsAboutToBeRemoved { first: j, count: 1 },
                    );
                    self.rows.borrow_mut().remove(j);
                    self.change(engine, ModelChange::RowsRemoved { first: j, count: 1 });
                }
            }
            // Where in `new` each kept row goes, in the order shown.
            let mut place: Vec<usize> = target.into_iter().flatten().collect();
            self.reorder(engine, &mut place);
            // The kept rows are in the new order now, so a row of a new
            // key goes at its place in `new`: every row before it is there.
            let mut kept = place.iter().peekable();
            let mut i = 0;
            while i < new.len() {
                if kept.peek().is_some_and(|&&p| p == i) {
                    kept.next();
                    i += 1;
                    continue;
                }
                let first = i;
                while i < new.len() && kept.peek().is_none_or(|&&p| p != i) {
                    i += 1;
                }
                let count = i - first;
                self.change(engine, ModelChange::RowsAboutToBeInserted { first, count });
                let made = self.rows_of(
                    new[first..i]
                        .iter()
                        .cloned()
                        .zip(new_keys[first..i].iter().cloned()),
                );
                self.rows.borrow_mut().splice(first..first, made);
                self.change(engine, ModelChange::RowsInserted { first, count });
            }
            for &p in &place {
                let old = self.rows.borrow()[p].value.clone();
                if !engine.same_model(&engine.model_snapshot(&old), &new[p]) {
                    self.rows.borrow_mut()[p].value = new[p].clone();
                    self.change(
                        engine,
                        ModelChange::DataChanged {
                            first: p,
                            count: 1,
                            roles: Vec::new(),
                        },
                    );
                }
            }
        });
    }

    /// Brings the kept rows into the order of `place` with the fewest
    /// moves: the rows of a longest increasing run of `place` stay where
    /// they are, and each other row is moved, in the order of where it
    /// goes, to just before the first row in place that goes after it.
    fn reorder(&self, engine: &Engine, place: &mut Vec<usize>) {
        if place.is_sorted() {
            return;
        }
        let mut settled = vec![false; place.iter().max().map_or(0, |m| m + 1)];
        let stay = increasing(place);
        let mut moving = Vec::new();
        for (i, &p) in place.iter().enumerate() {
            if stay[i] {
                settled[p] = true;
            } else {
                moving.push(p);
            }
        }
        moving.sort_unstable();
        for p in moving {
            let Some(from) = place.iter().position(|&q| q == p) else {
                continue;
            };
            place.remove(from);
            let to = place
                .iter()
                .position(|&q| settled[q] && q > p)
                .unwrap_or(place.len());
            place.insert(to, p);
            settled[p] = true;
            {
                let mut rows = self.rows.borrow_mut();
                let row = rows.remove(from);
                rows.insert(to, row);
            }
            if from != to {
                self.change(engine, ModelChange::RowsMoved { from, to, count: 1 });
            }
        }
    }

    fn change(&self, engine: &Engine, change: ModelChange) {
        self.listeners.emit(engine, &change);
    }
}

/// Which items of `xs` make a longest strictly increasing run of it, found
/// by patience sorting in O(n log n).
fn increasing(xs: &[usize]) -> Vec<bool> {
    // The index of the last item of the best run of each length so far,
    // and each item's item before it in its run.
    let mut ends: Vec<usize> = Vec::new();
    let mut before: Vec<Option<usize>> = vec![None; xs.len()];
    for (i, &x) in xs.iter().enumerate() {
        let at = ends.partition_point(|&e| xs[e] < x);
        before[i] = at.checked_sub(1).map(|a| ends[a]);
        if at == ends.len() {
            ends.push(i);
        } else {
            ends[at] = i;
        }
    }
    let mut stay = vec![false; xs.len()];
    let mut at = ends.last().copied();
    while let Some(i) = at {
        stay[i] = true;
        at = before[i];
    }
    stay
}

impl ItemModel for KeyedRows {
    fn row_count(&self) -> usize {
        self.rows.borrow().len()
    }

    fn role_names(&self) -> Vec<Atom> {
        Vec::new()
    }

    fn data(&self, engine: &Engine, row: usize, role: &str) -> Value {
        let Some(v) = self.rows.borrow().get(row).map(|r| r.value.clone()) else {
            return Value::Undefined;
        };
        match &v {
            Value::Object(o) => engine.read_by_name(*o, role),
            _ => engine
                .fields(&v)
                .and_then(|f| f.into_iter().find(|(k, _)| k == role))
                .map(|(_, v)| v)
                .unwrap_or_default(),
        }
    }

    fn row_key(&self, row: usize) -> RowKey {
        self.rows.borrow().get(row).map_or(RowKey::MAX, |r| r.id)
    }

    fn model_data(&self, _: &Engine, row: usize) -> Option<Value> {
        self.rows.borrow().get(row).map(|r| r.value.clone())
    }

    fn subscribe(&self, listener: ModelListener) -> u64 {
        self.listeners.add(listener)
    }

    fn unsubscribe(&self, id: u64) {
        self.listeners.remove(id);
    }
}

/// What a view that may be keyed holds of the model it shows.
#[derive(Debug, Default)]
pub struct KeyedSource {
    held: RefCell<Option<Held>>,
}

#[derive(Debug)]
struct Held {
    rows: Rc<KeyedRows>,
    key: Atom,
    delegate: Option<ObjectId>,
}

/// What [`KeyedSource::model`] made of a model.
pub enum Keyed {
    /// The view is not keyed, or the model is not a list: it makes the
    /// model as it would.
    No,
    /// The rows shown were brought to the new list in place: the view
    /// keeps its source.
    Updated,
    /// A model to show from now on.
    New(Rc<dyn ItemModel>),
}

impl std::fmt::Debug for Keyed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Keyed::No => "No",
            Keyed::Updated => "Updated",
            Keyed::New(_) => "New",
        })
    }
}

impl KeyedSource {
    /// The view shows rows it matches by key: it was given a `key` and a
    /// list.
    pub fn is_keyed(&self) -> bool {
        self.held.borrow().is_some()
    }

    /// What `value` is to the view `view` given `key` and `delegate`. With
    /// `remake` the rows shown are not kept, as when the view moved.
    pub fn model(
        &self,
        engine: &Engine,
        key: &str,
        value: &Value,
        delegate: Option<ObjectId>,
        view: Option<ObjectId>,
        remake: bool,
    ) -> Keyed {
        let rows = match engine.plain(value) {
            Value::List(l) if !key.is_empty() => l.to_vec(),
            _ => {
                self.held.borrow_mut().take();
                return Keyed::No;
            }
        };
        let kept = {
            let held = self.held.borrow();
            held.as_ref()
                .filter(|h| !remake && *h.key == *key && h.delegate == delegate)
                .map(|h| h.rows.clone())
        };
        if let Some(shown) = kept {
            shown.update(engine, rows);
            return Keyed::Updated;
        }
        let made = Rc::new(KeyedRows::new(engine, key, rows, view));
        *self.held.borrow_mut() = Some(Held {
            rows: made.clone(),
            key: Atom::from(key),
            delegate,
        });
        Keyed::New(made)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(ids: &[&str]) -> Vec<Value> {
        ids.iter()
            .map(|id| Value::map([("id", Value::from(*id))]))
            .collect()
    }

    /// The moves a view hears when the rows `from` are given as `to`, and
    /// the keys shown after.
    fn moves(from: &[&str], to: &[&str]) -> (usize, Vec<String>) {
        let engine = Engine::new();
        let shown = KeyedRows::new(&engine, "id", rows(from), None);
        let moved = Rc::new(Cell::new(0));
        let heard = moved.clone();
        shown.subscribe(Rc::new(move |_, change| {
            if matches!(change, ModelChange::RowsMoved { .. }) {
                heard.set(heard.get() + 1);
            }
        }));
        shown.update(&engine, rows(to));
        let keys = shown
            .rows
            .borrow()
            .iter()
            .map(|r| r.key[1..].to_string())
            .collect();
        (moved.get(), keys)
    }

    #[test]
    fn a_row_moved_from_the_front_to_the_end_is_one_move() {
        let to = ["b", "c", "d", "e", "a"];
        assert_eq!(
            moves(&["a", "b", "c", "d", "e"], &to),
            (1, to.map(String::from).to_vec())
        );
    }

    #[test]
    fn a_reversed_list_is_one_move_fewer_than_its_rows() {
        let to = ["e", "d", "c", "b", "a"];
        assert_eq!(
            moves(&["a", "b", "c", "d", "e"], &to),
            (4, to.map(String::from).to_vec())
        );
    }

    #[test]
    fn rows_in_their_order_are_not_moved() {
        let to = ["a", "x", "c", "e"];
        assert_eq!(
            moves(&["a", "b", "c", "d", "e"], &to),
            (0, to.map(String::from).to_vec())
        );
    }

    #[test]
    fn rows_kept_removed_and_new_end_in_the_new_order() {
        let to = ["f", "d", "a", "g", "e", "b"];
        assert_eq!(
            moves(&["a", "b", "c", "d", "e"], &to),
            (2, to.map(String::from).to_vec())
        );
    }

    #[test]
    fn every_order_of_five_rows_is_reached_in_the_fewest_moves() {
        fn orders(of: Vec<&'static str>) -> Vec<Vec<&'static str>> {
            if of.len() <= 1 {
                return vec![of];
            }
            let mut all = Vec::new();
            for i in 0..of.len() {
                let mut rest = of.clone();
                let first = rest.remove(i);
                for mut o in orders(rest) {
                    o.insert(0, first);
                    all.push(o);
                }
            }
            all
        }
        let from = ["a", "b", "c", "d", "e"];
        for to in orders(from.to_vec()) {
            // The longest run of rows already in order, counted plainly.
            let place: Vec<usize> = from
                .iter()
                .map(|k| to.iter().position(|t| t == k).unwrap())
                .collect();
            let mut best = vec![1; place.len()];
            for i in 0..place.len() {
                for j in 0..i {
                    if place[j] < place[i] {
                        best[i] = best[i].max(best[j] + 1);
                    }
                }
            }
            let fewest = place.len() - best.iter().max().unwrap();
            let want: Vec<String> = to.iter().map(|t| t.to_string()).collect();
            assert_eq!(moves(&from, &to), (fewest, want), "{to:?}");
        }
    }
}
