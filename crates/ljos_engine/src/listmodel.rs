//! `ListModel` and `ListElement` (QtQml.Models).
//!
//! A `ListModel` is rows of role values. A role gets the type of the
//! first value it is given and keeps it, as in Qt: later values of
//! another type are refused (`set`, `append`) or converted
//! (`setProperty`), with Qt's warnings.
//!
//! `ListElement`s are not objects with properties: the object creator
//! reads them from the IR ([`declare_rows`]) and they become rows. A list
//! of elements given to a role becomes a `ListModel` of its own.
//!
//! `get(i)` hands out a live object for the row: reading it reads the
//! row, writing it writes the row. It is made on first use and kept
//! until the row goes.
//!
//! What changes is said with `QAbstractItemModel`'s signals, as Qt's
//! `ListModel` says it (measured on Qt 6.11): a role is named by its
//! index among the model's roles, a clear is a removal of every row, and
//! the reset, layout, column and header signals are there but never sent.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::atom::Atom;
use crate::builtins::QtObject;
use crate::cx::Cx;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{ObjectId, PropIndex};
use crate::meta::MetaRef;
use crate::model::{
    ItemModel, ModelChange, ModelListener, ModelListeners, ModelRow, RowKey, RowKind, row_shape,
};
use crate::object::QmlType;
use crate::value::Value;
use crate::{QmlObject, WriteKind, qml_impl};

/// `ListModel`.
#[derive(QmlObject)]
#[qml(module = "QtQml.Models", name = "ListModel", extends = QtObject)]
pub struct ListModel {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, readonly)]
    pub count: i32,
    /// Kept for source compatibility: roles here may always be added
    /// after the fact, and keep their type as without it.
    #[qml(notify)]
    pub dynamic_roles: bool,
    #[qml(skip)]
    state: Rc<ListState>,
}

impl Default for ListModel {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            count: 0,
            dynamic_roles: false,
            state: ListState::new(),
        }
    }
}

impl std::fmt::Debug for ListModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListModel")
            .field("count", &self.count)
            .finish_non_exhaustive()
    }
}

/// `ListElement`: a row of the `ListModel` it is written in.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQml.Models", name = "ListElement", extends = QtObject)]
pub struct ListElement {
    #[qml(base)]
    pub object: QtObject,
}

/// What a role holds. Set by the first value the role is given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoleKind {
    String,
    Number,
    Bool,
    /// A nested `ListModel`.
    List,
    /// An engine object.
    Object,
    /// Anything else (a JS object): kept as it is.
    Variant,
}

impl RoleKind {
    /// `None` for `null` and `undefined`, which make no role.
    fn of(engine: &Engine, v: &Value) -> Option<RoleKind> {
        Some(match v {
            Value::Undefined | Value::Null => return None,
            Value::String(_) | Value::Url(_) => RoleKind::String,
            Value::Int(_) | Value::Real(_) | Value::Enum(_) => RoleKind::Number,
            Value::Bool(_) => RoleKind::Bool,
            Value::List(_) => RoleKind::List,
            Value::Object(o) if is_list_model(engine, *o) => RoleKind::List,
            Value::Object(_) => RoleKind::Object,
            _ => RoleKind::Variant,
        })
    }

    /// Qt's name for the type, as its warnings print it.
    fn name(self) -> &'static str {
        match self {
            RoleKind::String => "String",
            RoleKind::Number => "Number",
            RoleKind::Bool => "Bool",
            RoleKind::List => "List",
            RoleKind::Object => "QObject",
            RoleKind::Variant => "VariantMap",
        }
    }

    /// What a row reads for a role it was never given.
    fn default(self) -> Value {
        match self {
            RoleKind::String => Value::string(""),
            RoleKind::Number => Value::Real(0.0),
            RoleKind::Bool => Value::Bool(false),
            RoleKind::List | RoleKind::Object | RoleKind::Variant => Value::Undefined,
        }
    }
}

fn is_list_model(engine: &Engine, obj: ObjectId) -> bool {
    engine
        .meta_of(obj)
        .is_some_and(|m| m.inherits(<ListModel as QmlType>::static_meta()))
}

#[derive(Debug)]
struct Role {
    name: Atom,
    kind: RoleKind,
}

struct ListRow {
    key: RowKey,
    /// By role; may be shorter than the role list, the rest being
    /// defaults.
    values: Vec<Value>,
    /// The object `get()` returned for the row.
    element: Option<ObjectId>,
}

/// Who is setting a value, which decides what a type mismatch does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    /// `append`, `insert`, `set`: warn and leave the role alone.
    Object,
    /// `setProperty`: warn and convert.
    Property,
    /// A write to a `get()` object or from a delegate: leave it alone.
    Write,
}

/// The rows of a `ListModel`, shared with the views that show it.
pub(crate) struct ListState {
    this: Weak<ListState>,
    /// The `ListModel` object, once anything has reached the state
    /// through it.
    me: Cell<Option<ObjectId>>,
    roles: RefCell<Vec<Role>>,
    rows: RefCell<Vec<ListRow>>,
    next_key: Cell<RowKey>,
    listeners: ModelListeners,
}

impl ListState {
    fn new() -> Rc<ListState> {
        Rc::new_cyclic(|this| ListState {
            this: this.clone(),
            me: Cell::new(None),
            roles: RefCell::new(Vec::new()),
            rows: RefCell::new(Vec::new()),
            next_key: Cell::new(0),
            listeners: ModelListeners::default(),
        })
    }

    /// The state of the `ListModel` `obj`.
    fn of(engine: &Engine, obj: ObjectId) -> Option<Rc<ListState>> {
        let state = engine.with::<ListModel, _>(obj, |m| m.state.clone()).ok()?;
        state.me.set(Some(obj));
        Some(state)
    }

    fn len(&self) -> usize {
        self.rows.borrow().len()
    }

    fn names(&self) -> Vec<Atom> {
        self.roles.borrow().iter().map(|r| r.name.clone()).collect()
    }

    fn role_index(&self, name: &str) -> Option<usize> {
        self.roles.borrow().iter().position(|r| r.name == name)
    }

    fn value(&self, row: usize, role: usize) -> Value {
        let rows = self.rows.borrow();
        match rows.get(row).and_then(|r| r.values.get(role)) {
            Some(v) => v.clone(),
            None => self
                .roles
                .borrow()
                .get(role)
                .map(|r| r.kind.default())
                .unwrap_or_default(),
        }
    }

    fn warn(&self, engine: &Engine, what: &str) {
        let at = self
            .me
            .get()
            .and_then(|o| engine.location_of(o))
            .unwrap_or_default();
        engine.warn(format!("{at}QML ListModel: {what}"));
    }

    /// Where `value` goes when given to the role `name`, and as what:
    /// the role's index and the value to store, or `None` when the role
    /// is to be left alone. Makes the role when the model has none of
    /// that name.
    fn resolve(
        &self,
        engine: &Engine,
        name: &str,
        value: Value,
        source: Source,
    ) -> Option<(usize, Value)> {
        let existing = self.role_index(name);
        let Some(kind) = RoleKind::of(engine, &value) else {
            return match (&value, source, existing) {
                (Value::Null, Source::Object, _) => {
                    engine.warn(format!(
                        "<Unknown File>: {name} is null. Adding an object with a null member does \
                         not create a role for it."
                    ));
                    None
                }
                (Value::Null, Source::Property, _) => {
                    engine.warn(
                        "<Unknown File>: Can't create role for unsupported data type".to_owned(),
                    );
                    None
                }
                // `undefined` takes the role back to its default.
                (Value::Undefined, _, Some(i)) => Some((i, self.roles.borrow()[i].kind.default())),
                _ => None,
            };
        };
        let Some(i) = existing else {
            if source == Source::Write {
                return None;
            }
            let mut roles = self.roles.borrow_mut();
            roles.push(Role {
                name: Atom::intern(name),
                kind,
            });
            let i = roles.len() - 1;
            drop(roles);
            return Some((i, self.stored(engine, value)));
        };
        let has = self.roles.borrow()[i].kind;
        if has == kind {
            return Some((i, self.stored(engine, value)));
        }
        if source == Source::Write {
            return None;
        }
        engine.warn(format!(
            "<Unknown File>: Can't assign to existing role '{name}' of different type [{} -> {}]",
            kind.name(),
            has.name()
        ));
        if source == Source::Object {
            return None;
        }
        let converted = match has {
            RoleKind::String => Value::String(engine.to_js_string(&value)),
            RoleKind::Number => {
                let n = value.to_number();
                Value::Real(if n.is_nan() { 0.0 } else { n })
            }
            RoleKind::Bool => Value::Bool(value.to_boolean()),
            RoleKind::List | RoleKind::Object | RoleKind::Variant => return None,
        };
        Some((i, converted))
    }

    /// `value` as a row keeps it: numbers are JS numbers, and a list is
    /// a model of its own.
    fn stored(&self, engine: &Engine, value: Value) -> Value {
        match value {
            Value::Int(n) => Value::Real(f64::from(n)),
            Value::Enum(_) => Value::Real(value.to_number()),
            Value::List(items) => {
                let rows = items.iter().filter_map(|v| entries_of(engine, v)).collect();
                match self.me.get() {
                    Some(me) => nested_model(engine, me, rows),
                    None => Value::Undefined,
                }
            }
            v => v,
        }
    }

    /// Puts `v` at `role` of a row's values, filling what is before it.
    fn put(&self, values: &mut Vec<Value>, role: usize, v: Value) {
        let roles = self.roles.borrow();
        while values.len() <= role {
            let d = roles
                .get(values.len())
                .map(|r| r.kind.default())
                .unwrap_or_default();
            values.push(d);
        }
        values[role] = v;
    }

    /// Sends the signal `name` with what `args` makes, when anything
    /// listens: model indexes are made by script and most models are
    /// heard only by their views.
    fn tell(&self, engine: &Engine, name: &str, args: impl FnOnce(ObjectId) -> Vec<Value>) {
        let Some(me) = self.me.get() else { return };
        let Some(s) = <ListModel as QmlType>::static_meta().signal_index(name) else {
            return;
        };
        if engine.signal_connected(me, s) {
            let _ = engine.emit(me, s, &args(me));
        }
    }

    /// `first` to `last`, under no parent, as the row signals give them.
    fn tell_rows(&self, engine: &Engine, name: &str, first: usize, count: usize) {
        self.tell(engine, name, |_| {
            vec![
                crate::item_selection::invalid_index(engine),
                Value::Int(first as i32),
                Value::Int((first + count) as i32 - 1),
            ]
        });
    }

    /// After a change: the count, the views, then the signal `tell`
    /// sends, as one batch so what depends on several of them runs once
    /// and `countChanged` comes last, as in Qt.
    fn changed(
        &self,
        engine: &Engine,
        had_roles: usize,
        change: Option<ModelChange>,
        tell: impl FnOnce(),
    ) {
        engine.batch(|| {
            if let Some(me) = self.me.get() {
                let _ = engine.set_internal(me, ListModel::COUNT, self.len() as i32);
            }
            if self.roles.borrow().len() != had_roles {
                self.listeners.emit(engine, &ModelChange::RolesChanged);
            }
            if let Some(c) = change {
                self.listeners.emit(engine, &c);
            }
            tell();
        });
    }

    fn make_rows(&self, engine: &Engine, items: Vec<Vec<(String, Value)>>) -> Vec<ListRow> {
        items
            .into_iter()
            .map(|entries| {
                let mut values = Vec::new();
                for (name, v) in entries {
                    if let Some((i, v)) = self.resolve(engine, &name, v, Source::Object) {
                        self.put(&mut values, i, v);
                    }
                }
                let key = self.next_key.get();
                self.next_key.set(key + 1);
                ListRow {
                    key,
                    values,
                    element: None,
                }
            })
            .collect()
    }

    fn insert(&self, engine: &Engine, at: usize, items: Vec<Vec<(String, Value)>>) {
        if items.is_empty() {
            return;
        }
        let had = self.roles.borrow().len();
        let rows = self.make_rows(engine, items);
        let count = rows.len();
        self.listeners.emit(
            engine,
            &ModelChange::RowsAboutToBeInserted { first: at, count },
        );
        self.tell_rows(engine, "rowsAboutToBeInserted", at, count);
        self.rows.borrow_mut().splice(at..at, rows);
        self.changed(
            engine,
            had,
            Some(ModelChange::RowsInserted { first: at, count }),
            || self.tell_rows(engine, "rowsInserted", at, count),
        );
    }

    fn remove(&self, engine: &Engine, first: usize, count: usize) {
        self.listeners
            .emit(engine, &ModelChange::RowsAboutToBeRemoved { first, count });
        self.tell_rows(engine, "rowsAboutToBeRemoved", first, count);
        // A listener may have changed the model.
        let len = self.len();
        let first = first.min(len);
        let count = count.min(len - first);
        let gone: Vec<ListRow> = self.rows.borrow_mut().drain(first..first + count).collect();
        let roles = self.roles.borrow().len();
        self.changed(
            engine,
            roles,
            Some(ModelChange::RowsRemoved { first, count }),
            || {
                if count > 0 {
                    self.tell_rows(engine, "rowsRemoved", first, count);
                }
            },
        );
        for row in gone {
            self.drop_row(engine, row);
        }
    }

    /// What a row owned goes with it. Deferred: a handler may be running
    /// in one of them.
    fn drop_row(&self, engine: &Engine, row: ListRow) {
        if let Some(e) = row.element {
            engine.destroy(e);
        }
        for v in &row.values {
            if let Value::Object(o) = v
                && engine.owner(*o) == self.me.get()
                && is_list_model(engine, *o)
            {
                engine.destroy(*o);
            }
        }
    }

    fn move_rows(&self, engine: &Engine, from: usize, to: usize, count: usize) {
        // Qt names the row the moved ones go before, as it was.
        let before = if to > from { to + count } else { to };
        let tell = |name: &str| {
            self.tell(engine, name, |_| {
                let none = crate::item_selection::invalid_index(engine);
                vec![
                    none.clone(),
                    Value::Int(from as i32),
                    Value::Int((from + count) as i32 - 1),
                    none,
                    Value::Int(before as i32),
                ]
            });
        };
        tell("rowsAboutToBeMoved");
        {
            let mut rows = self.rows.borrow_mut();
            let moved: Vec<ListRow> = rows.drain(from..from + count).collect();
            rows.splice(to..to, moved);
        }
        let roles = self.roles.borrow().len();
        self.changed(
            engine,
            roles,
            Some(ModelChange::RowsMoved { from, to, count }),
            || tell("rowsMoved"),
        );
    }

    /// Gives `row` the values in `entries`. True when any was taken.
    fn set_fields(
        &self,
        engine: &Engine,
        row: usize,
        entries: Vec<(String, Value)>,
        source: Source,
    ) -> bool {
        let had = self.roles.borrow().len();
        let mut changed = Vec::new();
        let mut ids = Vec::new();
        for (name, v) in entries {
            let Some((i, v)) = self.resolve(engine, &name, v, source) else {
                continue;
            };
            // A write of the value the role already has is still a
            // change Qt tells its views of (measured on Qt 6.11: a
            // DelegateChooser remakes the row's delegate), so there is
            // no same-value shortcut here. Bindings do not re-run for
            // it: the row objects' writes cut off at equality.
            let old = self.value(row, i);
            let new_obj = v.as_object();
            let mut values = match self.rows.borrow_mut().get_mut(row) {
                Some(r) => std::mem::take(&mut r.values),
                None => return false,
            };
            self.put(&mut values, i, v);
            if let Some(r) = self.rows.borrow_mut().get_mut(row) {
                r.values = values;
            }
            if let Value::Object(o) = old
                && Some(o) != new_obj
                && engine.owner(o) == self.me.get()
                && is_list_model(engine, o)
            {
                engine.destroy(o);
            }
            changed.push(Atom::intern(&name));
            ids.push(Value::Int(i as i32));
        }
        let any = !changed.is_empty();
        if any {
            self.sync_element(engine, row);
        }
        if any || self.roles.borrow().len() != had {
            self.changed(
                engine,
                had,
                any.then_some(ModelChange::DataChanged {
                    first: row,
                    count: 1,
                    roles: changed,
                }),
                || {
                    if any {
                        self.tell(engine, "dataChanged", |me| {
                            let at = crate::item_selection::make_index(
                                engine,
                                Some(me),
                                &[(row as i32, 0)],
                            );
                            vec![at.clone(), at, Value::list(ids)]
                        });
                    }
                },
            );
        }
        any
    }

    fn clear(&self, engine: &Engine) {
        let count = self.len();
        if count == 0 {
            return;
        }
        self.listeners.emit(
            engine,
            &ModelChange::RowsAboutToBeRemoved { first: 0, count },
        );
        self.tell_rows(engine, "rowsAboutToBeRemoved", 0, count);
        let gone: Vec<ListRow> = std::mem::take(&mut *self.rows.borrow_mut());
        if gone.is_empty() {
            return;
        }
        let roles = self.roles.borrow().len();
        self.changed(
            engine,
            roles,
            Some(ModelChange::RowsRemoved {
                first: 0,
                count: gone.len(),
            }),
            || self.tell_rows(engine, "rowsRemoved", 0, gone.len()),
        );
        for row in gone {
            self.drop_row(engine, row);
        }
    }

    /// The live object for `row`, made on first use.
    fn element(&self, engine: &Engine, row: usize) -> Value {
        let (existing, key) = match self.rows.borrow().get(row) {
            Some(r) => (r.element, r.key),
            None => return Value::Undefined,
        };
        if let Some(e) = existing.filter(|e| engine.is_alive(*e)) {
            self.sync_element(engine, row);
            return Value::Object(e);
        }
        let shape = row_shape(RowKind::Element, &self.names());
        let Ok(e) = engine.create_with(
            shape.meta.clone(),
            Box::new(ModelRow::default()),
            self.me.get(),
        ) else {
            return Value::Undefined;
        };
        let this = self.this.clone();
        engine.set_write_hook(
            e,
            Some(Rc::new(
                move |engine: &Engine, prop: PropIndex, v: &Value| {
                    let Some(state) = this.upgrade() else {
                        return true;
                    };
                    let name = engine
                        .meta_of(e)
                        .and_then(|m| m.property(prop).map(|p| p.name.to_string()));
                    let row = state.rows.borrow().iter().position(|r| r.key == key);
                    if let (Some(name), Some(row)) = (name, row) {
                        state.set_fields(engine, row, vec![(name, v.clone())], Source::Write);
                    }
                    true
                },
            )),
        );
        if let Some(r) = self.rows.borrow_mut().get_mut(row) {
            r.element = Some(e);
        }
        self.sync_element(engine, row);
        Value::Object(e)
    }

    /// Brings the `get()` object of `row` up to date with the row, and
    /// with the roles the model has gained since it was made.
    fn sync_element(&self, engine: &Engine, row: usize) {
        let element = self.rows.borrow().get(row).and_then(|r| r.element);
        let Some(e) = element.filter(|e| engine.is_alive(*e)) else {
            return;
        };
        let shape = row_shape(RowKind::Element, &self.names());
        let current = engine
            .meta_of(e)
            .is_some_and(|m| MetaRef::ptr_eq(&m, &shape.meta));
        if !current && engine.extend_meta(e, shape.meta.clone()).is_err() {
            return;
        }
        for (name, p) in &shape.roles {
            if let Some(i) = self.role_index(name) {
                let _ = engine.write_internal(e, *p, self.value(row, i), WriteKind::Imperative);
            }
        }
    }
}

impl ItemModel for ListState {
    fn row_count(&self) -> usize {
        self.len()
    }

    fn role_names(&self) -> Vec<Atom> {
        self.names()
    }

    fn data(&self, _: &Engine, row: usize, role: &str) -> Value {
        match self.role_index(role) {
            Some(i) if row < self.len() => self.value(row, i),
            _ => Value::Undefined,
        }
    }

    fn set_data(&self, engine: &Engine, row: usize, role: &str, value: &Value) -> bool {
        row < self.len()
            && self.set_fields(
                engine,
                row,
                vec![(role.to_owned(), value.clone())],
                Source::Write,
            )
    }

    fn row_key(&self, row: usize) -> RowKey {
        self.rows.borrow().get(row).map_or(RowKey::MAX, |r| r.key)
    }

    fn model_data(&self, _: &Engine, row: usize) -> Option<Value> {
        // Qt gives the delegates of a one-role model that role as
        // `modelData`.
        (self.roles.borrow().len() == 1 && row < self.len()).then(|| self.value(row, 0))
    }

    fn subscribe(&self, listener: ModelListener) -> u64 {
        self.listeners.add(listener)
    }

    fn unsubscribe(&self, id: u64) {
        self.listeners.remove(id);
    }
}

/// The roles and values of a JS object given to `append` and the like.
fn entries_of(engine: &Engine, v: &Value) -> Option<Vec<(String, Value)>> {
    match v {
        Value::Js(_) => Some(engine.js_object_entries(v)),
        _ => None,
    }
}

/// An index argument: JS numbers are truncated, as Qt does.
fn index_of(v: &Value) -> Option<i64> {
    let n = v.as_number()?;
    n.is_finite().then(|| n.trunc() as i64)
}

/// Gives the `ListModel` `model` the rows it is declared with. Nothing
/// is told: nobody can be listening yet.
pub(crate) fn declare_rows(engine: &Engine, model: ObjectId, rows: Vec<Vec<(String, Value)>>) {
    let Some(state) = ListState::of(engine, model) else {
        return;
    };
    let rows = state.make_rows(engine, rows);
    state.rows.borrow_mut().extend(rows);
    let _ = engine.write_internal(
        model,
        ListModel::COUNT.index,
        Value::Int(state.len() as i32),
        WriteKind::Initial,
    );
}

/// A `ListModel` holding `rows`, owned by the model `owner`: what a list
/// in a role is.
pub(crate) fn nested_model(
    engine: &Engine,
    owner: ObjectId,
    rows: Vec<Vec<(String, Value)>>,
) -> Value {
    match engine.create_native_incomplete(ListModel::default(), Some(owner)) {
        Ok(id) => {
            declare_rows(engine, id, rows);
            Value::Object(id)
        }
        Err(_) => Value::Undefined,
    }
}

#[qml_impl]
impl ListModel {
    #[qml(signal, name = "dataChanged")]
    fn data_changed(top_left: Value, bottom_right: Value, roles: Value);

    #[qml(signal, name = "headerDataChanged")]
    fn header_data_changed(orientation: i32, first: i32, last: i32);

    #[qml(signal, name = "layoutAboutToBeChanged")]
    fn layout_about_to_be_changed(parents: Value, hint: i32);

    #[qml(signal, name = "layoutChanged")]
    fn layout_changed(parents: Value, hint: i32);

    #[qml(signal, name = "modelAboutToBeReset")]
    fn model_about_to_be_reset();

    #[qml(signal, name = "modelReset")]
    fn model_reset();

    #[qml(signal, name = "rowsAboutToBeInserted")]
    fn rows_about_to_be_inserted(parent: Value, first: i32, last: i32);

    #[qml(signal, name = "rowsInserted")]
    fn rows_inserted(parent: Value, first: i32, last: i32);

    #[qml(signal, name = "rowsAboutToBeRemoved")]
    fn rows_about_to_be_removed(parent: Value, first: i32, last: i32);

    #[qml(signal, name = "rowsRemoved")]
    fn rows_removed(parent: Value, first: i32, last: i32);

    #[qml(signal, name = "rowsAboutToBeMoved")]
    fn rows_about_to_be_moved(
        source_parent: Value,
        source_start: i32,
        source_end: i32,
        destination_parent: Value,
        destination_row: i32,
    );

    #[qml(signal, name = "rowsMoved")]
    fn rows_moved(
        source_parent: Value,
        source_start: i32,
        source_end: i32,
        destination_parent: Value,
        destination_row: i32,
    );

    #[qml(signal, name = "columnsAboutToBeInserted")]
    fn columns_about_to_be_inserted(parent: Value, first: i32, last: i32);

    #[qml(signal, name = "columnsInserted")]
    fn columns_inserted(parent: Value, first: i32, last: i32);

    #[qml(signal, name = "columnsAboutToBeRemoved")]
    fn columns_about_to_be_removed(parent: Value, first: i32, last: i32);

    #[qml(signal, name = "columnsRemoved")]
    fn columns_removed(parent: Value, first: i32, last: i32);

    #[qml(signal, name = "columnsAboutToBeMoved")]
    fn columns_about_to_be_moved(
        source_parent: Value,
        source_start: i32,
        source_end: i32,
        destination_parent: Value,
        destination_column: i32,
    );

    #[qml(signal, name = "columnsMoved")]
    fn columns_moved(
        source_parent: Value,
        source_start: i32,
        source_end: i32,
        destination_parent: Value,
        destination_column: i32,
    );

    /// The model index of the row: what a selection model or a view is
    /// handed; the invalid index out of range.
    #[qml(invokable)]
    fn index(cx: &Cx, row: Value, column: Value) -> Value {
        crate::item_selection::list_index(cx.engine, cx.this, &row, &column)
    }

    /// The row at `index` as a live object; `undefined` out of range.
    #[qml(invokable)]
    fn get(cx: &Cx, index: Value) -> Value {
        let Some(state) = ListState::of(cx.engine, cx.this) else {
            return Value::Undefined;
        };
        match index_of(&index) {
            Some(i) if i >= 0 && (i as usize) < state.len() => state.element(cx.engine, i as usize),
            _ => Value::Undefined,
        }
    }

    /// Adds a row from a JS object, or one per object of an array.
    #[qml(invokable)]
    fn append(cx: &Cx, item: Value) {
        let Some(state) = ListState::of(cx.engine, cx.this) else {
            return;
        };
        match rows_of(cx.engine, &item) {
            Some(rows) => state.insert(cx.engine, state.len(), rows),
            None => state.warn(cx.engine, "append: value is not an object"),
        }
    }

    #[qml(invokable)]
    fn insert(cx: &Cx, index: Value, item: Value) {
        let Some(state) = ListState::of(cx.engine, cx.this) else {
            return;
        };
        let at = index_of(&index).unwrap_or(-1);
        if at < 0 || at as usize > state.len() {
            state.warn(cx.engine, &format!("insert: index {at} out of range"));
            return;
        }
        match rows_of(cx.engine, &item) {
            Some(rows) => state.insert(cx.engine, at as usize, rows),
            None => state.warn(cx.engine, "insert: value is not an object"),
        }
    }

    /// Removes `count` rows (one when not given) from `index`.
    #[qml(invokable)]
    fn remove(cx: &Cx, index: Value, count: Value) {
        let Some(state) = ListState::of(cx.engine, cx.this) else {
            return;
        };
        let first = index_of(&index).unwrap_or(-1);
        let n = match count {
            Value::Undefined => 1,
            c => index_of(&c).unwrap_or(0),
        };
        let len = state.len() as i64;
        if first < 0 || n <= 0 || first + n > len {
            state.warn(
                cx.engine,
                &format!(
                    "remove: indices [{first} - {}] out of range [0 - {len}]",
                    first + n
                ),
            );
            return;
        }
        state.remove(cx.engine, first as usize, n as usize);
    }

    /// Moves `count` rows from `from` so that the first of them ends up
    /// at `to`.
    #[qml(invokable, name = "move")]
    fn move_rows(cx: &Cx, from: Value, to: Value, count: Value) {
        let Some(state) = ListState::of(cx.engine, cx.this) else {
            return;
        };
        let (from, to, n) = (
            index_of(&from).unwrap_or(-1),
            index_of(&to).unwrap_or(-1),
            index_of(&count).unwrap_or(-1),
        );
        let len = state.len() as i64;
        if n < 0 || from < 0 || to < 0 || from + n > len || to + n > len {
            state.warn(cx.engine, "move: out of range");
            return;
        }
        if n == 0 || from == to {
            return;
        }
        state.move_rows(cx.engine, from as usize, to as usize, n as usize);
    }

    /// Changes the roles of the row at `index` that `item` names. At
    /// `count` it appends.
    #[qml(invokable)]
    fn set(cx: &Cx, index: Value, item: Value) {
        let Some(state) = ListState::of(cx.engine, cx.this) else {
            return;
        };
        let at = index_of(&index).unwrap_or(-1);
        if at < 0 || at as usize > state.len() {
            state.warn(cx.engine, &format!("set: index {at} out of range"));
            return;
        }
        let Some(entries) = entries_of(cx.engine, &item) else {
            state.warn(cx.engine, "set: value is not an object");
            return;
        };
        if at as usize == state.len() {
            state.insert(cx.engine, at as usize, vec![entries]);
        } else {
            state.set_fields(cx.engine, at as usize, entries, Source::Object);
        }
    }

    #[qml(invokable, name = "setProperty")]
    fn set_property(cx: &Cx, index: Value, property: Value, value: Value) {
        let Some(state) = ListState::of(cx.engine, cx.this) else {
            return;
        };
        let at = index_of(&index).unwrap_or(-1);
        if at < 0 || at as usize >= state.len() {
            state.warn(cx.engine, &format!("set: index {at} out of range"));
            return;
        }
        let name = cx.engine.to_js_string(&property).to_string();
        state.set_fields(
            cx.engine,
            at as usize,
            vec![(name, value)],
            Source::Property,
        );
    }

    #[qml(invokable)]
    fn clear(cx: &Cx) {
        if let Some(state) = ListState::of(cx.engine, cx.this) {
            state.clear(cx.engine);
        }
    }

    /// The `ListElement`s were read from the IR; the objects made for
    /// them have nothing left to do.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let element = <ListElement as QmlType>::static_meta();
        for c in cx.engine.children(cx.this) {
            if cx.engine.meta_of(c).is_some_and(|m| m.inherits(element)) {
                cx.engine.destroy(c);
            }
        }
        Ok(())
    }
}

/// The rows an argument of `append` or `insert` stands for.
fn rows_of(engine: &Engine, item: &Value) -> Option<Vec<Vec<(String, Value)>>> {
    match item {
        Value::List(items) => items.iter().map(|v| entries_of(engine, v)).collect(),
        v => entries_of(engine, v).map(|e| vec![e]),
    }
}

impl Engine {
    /// The state of the `ListModel` `obj`, for a view.
    pub(crate) fn list_model_state(&self, obj: ObjectId) -> Option<Rc<dyn ItemModel>> {
        ListState::of(self, obj).map(|s| s as Rc<dyn ItemModel>)
    }
}

/// A role's value as a dev reload carries it.
#[derive(Clone, Debug, PartialEq)]
pub enum CarriedCell {
    Plain(Value),
    /// A nested model, by its rows.
    Nested(CarriedRows),
}

/// A model's rows as a dev reload carries them: by role name, as the
/// model made from the new file starts with no roles.
pub type CarriedRows = Vec<Vec<(String, CarriedCell)>>;

fn carried_rows(engine: &Engine, state: &ListState) -> CarriedRows {
    let names = state.names();
    (0..state.len())
        .map(|row| {
            let mut cells = Vec::with_capacity(names.len());
            for (role, name) in names.iter().enumerate() {
                let cell = match state.value(row, role) {
                    Value::Undefined | Value::Null => continue,
                    Value::Object(o) => match ListState::of(engine, o) {
                        Some(nested) if is_list_model(engine, o) => {
                            CarriedCell::Nested(carried_rows(engine, &nested))
                        }
                        // An object of the old tree means nothing in the
                        // new one.
                        _ => continue,
                    },
                    v => CarriedCell::Plain(v),
                };
                cells.push((name.to_string(), cell));
            }
            cells
        })
        .collect()
}

fn restored_rows(engine: &Engine, model: ObjectId, rows: CarriedRows) -> Vec<Vec<(String, Value)>> {
    rows.into_iter()
        .map(|cells| {
            cells
                .into_iter()
                .map(|(name, cell)| {
                    let v = match cell {
                        CarriedCell::Plain(v) => v,
                        CarriedCell::Nested(rows) => {
                            let rows = restored_rows(engine, model, rows);
                            nested_model(engine, model, rows)
                        }
                    };
                    (name, v)
                })
                .collect()
        })
        .collect()
}

/// The rows a script appended, removed or changed go on across a dev
/// reload. (When the file's `ListElement`s were edited, the engine does
/// not ask: the file wins.)
impl crate::Migrate for ListModel {
    const VERSION: u32 = 1;
    type State = CarriedRows;

    fn snapshot(engine: &Engine, obj: ObjectId) -> Option<CarriedRows> {
        let state = ListState::of(engine, obj)?;
        Some(carried_rows(engine, &state))
    }

    fn migrate(
        engine: &Engine,
        obj: ObjectId,
        _from: u32,
        rows: CarriedRows,
    ) -> Result<(), String> {
        let state = ListState::of(engine, obj).ok_or("not a ListModel")?;
        // The usual case, and views are not told of rows going and
        // coming.
        if carried_rows(engine, &state) == rows {
            return Ok(());
        }
        state.clear(engine);
        let rows = restored_rows(engine, obj, rows);
        state.insert(engine, 0, rows);
        Ok(())
    }
}
