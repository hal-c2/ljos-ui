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
            for i in 0..place.len() {
                let Some(m) = (i..place.len()).min_by_key(|&m| place[m]) else {
                    break;
                };
                if m == i {
                    continue;
                }
                place[i..=m].rotate_right(1);
                self.rows.borrow_mut()[i..=m].rotate_right(1);
                self.change(
                    engine,
                    ModelChange::RowsMoved {
                        from: m,
                        to: i,
                        count: 1,
                    },
                );
            }
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

    fn change(&self, engine: &Engine, change: ModelChange) {
        self.listeners.emit(engine, &change);
    }
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
