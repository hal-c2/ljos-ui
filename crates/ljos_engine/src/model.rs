//! The model interface every view reads through (ARCHITECTURE.md §8.4,
//! §12.1), the adapters that make a model of a plain value, and the row
//! object a delegate sees its row through.
//!
//! A view never looks at what `model:` was given. It asks
//! [`Engine::model_from_value`] for an [`ItemModel`] and listens to its
//! [`ModelChange`]s, so a number, a JS array, a list of objects, a
//! `ListModel` and a model written in Rust all drive the same code.
//!
//! A delegate reads its row through a row object: an engine object with
//! the properties `index`, `model` (itself), `modelData`, and one per
//! role. It is the context object of the delegate's context, which is
//! what makes the role names visible in the delegate, and `model.role =
//! v` is a property write that its write hook sends to the model.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use ljos_reactive::{NodeId, NodeKind, NodeState};

use crate::atom::Atom;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{ObjectId, PropIndex};
use crate::meta::{MetaBuilder, MetaRef, PropertyAccess, PropertyFlags};
use crate::object::{ObjectFlags, QmlType, WriteHook};
use crate::types::TypeTag;
use crate::value::Value;
use crate::{QmlObject, WriteKind};

/// Identifies a row for as long as it is in its model, whatever its
/// position: what a view keys its delegates, selection and scroll anchor
/// by.
pub type RowKey = u64;

/// What a model tells its views. Each is sent after the model changed,
/// but for the two that say "about to": those are sent first, while the
/// model still has (or does not yet have) the rows, so a view can look up
/// what is leaving. A model need not send them; a view that needs them
/// must also work without.
#[derive(Clone, Debug, PartialEq)]
pub enum ModelChange {
    /// `count` rows are about to start at `first`.
    RowsAboutToBeInserted { first: usize, count: usize },
    /// The `count` rows that start at `first` are about to go.
    RowsAboutToBeRemoved { first: usize, count: usize },
    /// `count` rows now start at `first`.
    RowsInserted { first: usize, count: usize },
    /// The `count` rows that started at `first` are gone.
    RowsRemoved { first: usize, count: usize },
    /// The `count` rows that started at `from` now start at `to` (an
    /// index into the model with those rows taken out).
    RowsMoved {
        from: usize,
        to: usize,
        count: usize,
    },
    /// Roles of `count` rows from `first` changed; no roles means any.
    DataChanged {
        first: usize,
        count: usize,
        roles: Vec<Atom>,
    },
    /// The set of roles grew.
    RolesChanged,
    /// Everything may be different.
    ModelReset,
}

/// Receives a model's changes.
pub type ModelListener = Rc<dyn Fn(&Engine, &ModelChange)>;

/// Where the delegates of a model's rows come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DelegateSource {
    /// The view makes one of its own `delegate` for each row it shows.
    View,
    /// Each row is an object that shows itself ([`ItemModel::row_object`]),
    /// as the items of Qt's `ObjectModel`: nothing is made or destroyed.
    Rows,
    /// The model brings the component, as Qt's `DelegateModel`; `None`
    /// while it has none, and then no row has a delegate.
    Component(Option<ObjectId>),
}

/// Rows of named roles. Implemented by `ListModel`, by the adapters in
/// this module, and by models written in Rust
/// ([`Engine::set_object_model`]).
pub trait ItemModel {
    fn row_count(&self) -> usize;

    /// The roles a row has, in a stable order. May grow
    /// ([`ModelChange::RolesChanged`]), never shrinks or reorders.
    fn role_names(&self) -> Vec<Atom>;

    /// `undefined` for a row or role the model does not have.
    fn data(&self, engine: &Engine, row: usize, role: &str) -> Value;

    /// False: the model is read-only, or refused the value.
    fn set_data(&self, engine: &Engine, row: usize, role: &str, value: &Value) -> bool {
        let _ = (engine, row, role, value);
        false
    }

    fn row_key(&self, row: usize) -> RowKey {
        row as RowKey
    }

    /// What `modelData` is in the row's delegate, for a model that has it.
    fn model_data(&self, engine: &Engine, row: usize) -> Option<Value> {
        let _ = (engine, row);
        None
    }

    /// The object the row is, for a model of objects: its properties are
    /// the roles.
    fn row_object(&self, row: usize) -> Option<ObjectId> {
        let _ = row;
        None
    }

    /// Calls `listener` after every change; the result is for
    /// [`ItemModel::unsubscribe`].
    fn subscribe(&self, listener: ModelListener) -> u64 {
        let _ = listener;
        0
    }

    fn unsubscribe(&self, id: u64) {
        let _ = id;
    }

    /// Where a view gets the delegates of the rows from.
    fn delegate_source(&self) -> DelegateSource {
        DelegateSource::View
    }

    /// A view is making `object` for row `row` of a model that brings
    /// its delegate ([`DelegateSource::Component`]): the object is there,
    /// and none of its bindings has run.
    fn delegate_allocated(&self, engine: &Engine, row: usize, object: ObjectId) {
        let _ = (engine, row, object);
    }

    /// A view made `object` for row `row` of a model that brings its
    /// delegate: it is complete.
    fn delegate_made(&self, engine: &Engine, row: usize, object: ObjectId) {
        let _ = (engine, row, object);
    }

    /// The delegate `object` a view made is about to be destroyed.
    fn delegate_gone(&self, engine: &Engine, object: ObjectId) {
        let _ = (engine, object);
    }

    /// The whole tree, for a model of a tree's top level
    /// ([`crate::ItemTree`]): what a `TreeView` shows.
    fn tree(&self) -> Option<Rc<dyn crate::ItemTree>> {
        None
    }
}

/// The subscription list of a model that changes.
#[derive(Default)]
pub struct ModelListeners {
    next: Cell<u64>,
    all: RefCell<Vec<(u64, ModelListener)>>,
}

impl std::fmt::Debug for ModelListeners {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelListeners")
            .field("len", &self.all.borrow().len())
            .finish()
    }
}

impl ModelListeners {
    pub fn add(&self, listener: ModelListener) -> u64 {
        let id = self.next.get() + 1;
        self.next.set(id);
        self.all.borrow_mut().push((id, listener));
        id
    }

    pub fn remove(&self, id: u64) {
        self.all.borrow_mut().retain(|(i, _)| *i != id);
    }

    pub fn is_empty(&self) -> bool {
        self.all.borrow().is_empty()
    }

    /// Tells every listener. A listener may subscribe or unsubscribe.
    pub fn emit(&self, engine: &Engine, change: &ModelChange) {
        let all: Vec<ModelListener> = self.all.borrow().iter().map(|(_, l)| l.clone()).collect();
        for l in all {
            l(engine, change);
        }
    }
}

/// A model written in Rust, as QML sees an item model: rows of named
/// roles and the row signals (ARCHITECTURE.md §17).
///
/// The model keeps its own data and says what changed through the
/// [`ModelSignals`] that [`Engine::set_list_model`] (or [`list_model`])
/// returns. Every view, `Repeater` and `Instantiator` given the object as
/// its `model` then follows it.
pub trait QmlListModel {
    fn row_count(&self) -> usize;

    /// The roles a row has, in a stable order: it may grow (say so with
    /// [`ModelSignals::roles_changed`]), never shrink or reorder.
    fn role_names(&self) -> Vec<Atom>;

    /// `undefined` for a row or role the model does not have.
    fn data(&self, row: usize, role: &str) -> Value;

    /// False: the model is read-only, or refused the value. A model that
    /// takes it says so with [`ModelSignals::data_changed`].
    fn set_data(&self, row: usize, role: &str, value: &Value) -> bool {
        let _ = (row, role, value);
        false
    }

    /// What identifies the row while it is in the model. The default, its
    /// position, is right only for a model whose rows never move; a view
    /// keeps a row's measured size and the scroll position by this.
    fn row_key(&self, row: usize) -> RowKey {
        row as RowKey
    }
}

/// The row signals of a [`QmlListModel`]: the model calls one around each
/// change, with the same meaning as the signals of Qt's item models.
#[derive(Clone, Debug, Default)]
pub struct ModelSignals(Rc<ModelListeners>);

impl ModelSignals {
    /// Before `count` rows go in at `first`.
    pub fn rows_about_to_be_inserted(&self, engine: &Engine, first: usize, count: usize) {
        self.0
            .emit(engine, &ModelChange::RowsAboutToBeInserted { first, count });
    }

    /// After `count` rows went in at `first`.
    pub fn rows_inserted(&self, engine: &Engine, first: usize, count: usize) {
        self.0
            .emit(engine, &ModelChange::RowsInserted { first, count });
    }

    /// Before the `count` rows from `first` go.
    pub fn rows_about_to_be_removed(&self, engine: &Engine, first: usize, count: usize) {
        self.0
            .emit(engine, &ModelChange::RowsAboutToBeRemoved { first, count });
    }

    /// After the `count` rows from `first` went.
    pub fn rows_removed(&self, engine: &Engine, first: usize, count: usize) {
        self.0
            .emit(engine, &ModelChange::RowsRemoved { first, count });
    }

    /// After the `count` rows from `from` moved to start at `to` (an index
    /// into the model with those rows taken out).
    pub fn rows_moved(&self, engine: &Engine, from: usize, to: usize, count: usize) {
        self.0
            .emit(engine, &ModelChange::RowsMoved { from, to, count });
    }

    /// After roles of `count` rows from `first` changed; no roles means
    /// any of them.
    pub fn data_changed(&self, engine: &Engine, first: usize, count: usize, roles: &[&str]) {
        let roles = roles.iter().map(|r| Atom::intern(r)).collect();
        self.0.emit(
            engine,
            &ModelChange::DataChanged {
                first,
                count,
                roles,
            },
        );
    }

    /// After the model gained roles.
    pub fn roles_changed(&self, engine: &Engine) {
        self.0.emit(engine, &ModelChange::RolesChanged);
    }

    /// After anything else: every row may be different.
    pub fn model_reset(&self, engine: &Engine) {
        self.0.emit(engine, &ModelChange::ModelReset);
    }

    /// Whether any view follows the model.
    pub fn is_watched(&self) -> bool {
        !self.0.is_empty()
    }
}

/// A [`QmlListModel`] as the model interface the views read.
struct NativeModel {
    model: Rc<dyn QmlListModel>,
    listeners: Rc<ModelListeners>,
}

impl ItemModel for NativeModel {
    fn row_count(&self) -> usize {
        self.model.row_count()
    }

    fn role_names(&self) -> Vec<Atom> {
        self.model.role_names()
    }

    fn data(&self, _: &Engine, row: usize, role: &str) -> Value {
        self.model.data(row, role)
    }

    fn set_data(&self, _: &Engine, row: usize, role: &str, value: &Value) -> bool {
        self.model.set_data(row, role, value)
    }

    fn row_key(&self, row: usize) -> RowKey {
        self.model.row_key(row)
    }

    fn model_data(&self, _: &Engine, row: usize) -> Option<Value> {
        // As Qt: a model with one role has that role as `modelData`.
        match self.model.role_names().as_slice() {
            [only] => Some(self.model.data(row, only)),
            _ => None,
        }
    }

    fn subscribe(&self, listener: ModelListener) -> u64 {
        self.listeners.add(listener)
    }

    fn unsubscribe(&self, id: u64) {
        self.listeners.remove(id);
    }
}

/// `model` as what a view reads, and the signals it is to send.
pub fn list_model(model: Rc<dyn QmlListModel>) -> (Rc<dyn ItemModel>, ModelSignals) {
    let listeners = Rc::new(ModelListeners::default());
    let signals = ModelSignals(listeners.clone());
    (Rc::new(NativeModel { model, listeners }), signals)
}

/// `model: 5`: rows with nothing but their number.
struct CountModel(usize);

impl ItemModel for CountModel {
    fn row_count(&self) -> usize {
        self.0
    }

    fn role_names(&self) -> Vec<Atom> {
        Vec::new()
    }

    fn data(&self, _: &Engine, _: usize, _: &str) -> Value {
        Value::Undefined
    }

    fn model_data(&self, _: &Engine, row: usize) -> Option<Value> {
        (row < self.0).then_some(Value::Int(row as i32))
    }
}

/// `model: ["a", "b"]`: each row is a value.
struct ValuesModel(Vec<Value>);

impl ItemModel for ValuesModel {
    fn row_count(&self) -> usize {
        self.0.len()
    }

    fn role_names(&self) -> Vec<Atom> {
        Vec::new()
    }

    fn data(&self, _: &Engine, _: usize, _: &str) -> Value {
        Value::Undefined
    }

    fn model_data(&self, _: &Engine, row: usize) -> Option<Value> {
        self.0.get(row).cloned()
    }
}

/// A list of objects, or one object: the properties are the roles.
struct ObjectsModel(Vec<ObjectId>);

impl ItemModel for ObjectsModel {
    fn row_count(&self) -> usize {
        self.0.len()
    }

    fn role_names(&self) -> Vec<Atom> {
        Vec::new()
    }

    fn data(&self, engine: &Engine, row: usize, role: &str) -> Value {
        match self.0.get(row) {
            Some(o) => engine.read_by_name(*o, role),
            None => Value::Undefined,
        }
    }

    fn set_data(&self, engine: &Engine, row: usize, role: &str, value: &Value) -> bool {
        self.0
            .get(row)
            .is_some_and(|o| engine.write_by_name(*o, role, value.clone()).is_ok())
    }

    fn model_data(&self, _: &Engine, row: usize) -> Option<Value> {
        self.0.get(row).map(|o| Value::Object(*o))
    }

    fn row_object(&self, row: usize) -> Option<ObjectId> {
        self.0.get(row).copied()
    }
}

/// A model whose rows are objects that show themselves: what Qt's
/// `ObjectModel` is to a view. The model does not own the objects.
#[derive(Default)]
pub struct ObjectRows {
    rows: RefCell<Vec<(RowKey, ObjectId)>>,
    next_key: Cell<RowKey>,
    listeners: ModelListeners,
}

impl std::fmt::Debug for ObjectRows {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjectRows")
            .field("rows", &self.rows.borrow().len())
            .finish()
    }
}

impl ObjectRows {
    pub fn len(&self) -> usize {
        self.rows.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.borrow().is_empty()
    }

    /// The objects, in row order.
    pub fn objects(&self) -> Vec<ObjectId> {
        self.rows.borrow().iter().map(|r| r.1).collect()
    }

    pub fn get(&self, row: usize) -> Option<ObjectId> {
        self.rows.borrow().get(row).map(|r| r.1)
    }

    pub fn index_of(&self, object: ObjectId) -> Option<usize> {
        self.rows.borrow().iter().position(|r| r.1 == object)
    }

    /// Puts `object` in as row `row`, or as the last one when there are
    /// fewer rows.
    pub fn insert(&self, engine: &Engine, row: usize, object: ObjectId) {
        let row = {
            let mut rows = self.rows.borrow_mut();
            let row = row.min(rows.len());
            let key = self.next_key.get() + 1;
            self.next_key.set(key);
            rows.insert(row, (key, object));
            row
        };
        let (first, count) = (row, 1);
        self.listeners
            .emit(engine, &ModelChange::RowsInserted { first, count });
    }

    /// Takes `count` rows from `first` out; the result is what they were.
    pub fn remove(&self, engine: &Engine, first: usize, count: usize) -> Vec<ObjectId> {
        let gone: Vec<ObjectId> = {
            let mut rows = self.rows.borrow_mut();
            let first = first.min(rows.len());
            let end = first.saturating_add(count).min(rows.len());
            rows.drain(first..end).map(|r| r.1).collect()
        };
        if !gone.is_empty() {
            let count = gone.len();
            self.listeners
                .emit(engine, &ModelChange::RowsRemoved { first, count });
        }
        gone
    }

    /// Moves `count` rows from `from` so that they start at `to`. False,
    /// and nothing moves, when either range is not in the model.
    pub fn move_rows(&self, engine: &Engine, from: usize, to: usize, count: usize) -> bool {
        {
            let mut rows = self.rows.borrow_mut();
            let fits = |at: usize| at.checked_add(count).is_some_and(|end| end <= rows.len());
            if count == 0 || !fits(from) || !fits(to) {
                return false;
            }
            if from == to {
                return true;
            }
            let moved: Vec<(RowKey, ObjectId)> = rows.drain(from..from + count).collect();
            rows.splice(to..to, moved);
        }
        self.listeners
            .emit(engine, &ModelChange::RowsMoved { from, to, count });
        true
    }

    pub fn clear(&self, engine: &Engine) {
        if self.rows.borrow().is_empty() {
            return;
        }
        self.rows.borrow_mut().clear();
        self.listeners.emit(engine, &ModelChange::ModelReset);
    }
}

impl ItemModel for ObjectRows {
    fn row_count(&self) -> usize {
        self.rows.borrow().len()
    }

    fn role_names(&self) -> Vec<Atom> {
        Vec::new()
    }

    fn data(&self, engine: &Engine, row: usize, role: &str) -> Value {
        match self.get(row) {
            Some(o) => engine.read_by_name(o, role),
            None => Value::Undefined,
        }
    }

    fn row_key(&self, row: usize) -> RowKey {
        self.rows.borrow().get(row).map_or(0, |r| r.0)
    }

    fn model_data(&self, _: &Engine, row: usize) -> Option<Value> {
        self.get(row).map(Value::Object)
    }

    fn row_object(&self, row: usize) -> Option<ObjectId> {
        self.get(row)
    }

    fn subscribe(&self, listener: ModelListener) -> u64 {
        self.listeners.add(listener)
    }

    fn unsubscribe(&self, id: u64) {
        self.listeners.remove(id);
    }

    fn delegate_source(&self) -> DelegateSource {
        DelegateSource::Rows
    }
}

/// Called with the row and the delegate a view made for it.
pub type DelegateMade = Rc<dyn Fn(&Engine, usize, ObjectId)>;
/// Called with a delegate that is about to be destroyed.
pub type DelegateGone = Rc<dyn Fn(&Engine, ObjectId)>;

/// A model together with the component of its rows' delegates: what Qt's
/// `DelegateModel` is to a view. Every view of it is told to start over
/// when either changes.
#[derive(Default)]
pub struct DelegateRows {
    model: RefCell<Option<(Rc<dyn ItemModel>, u64)>>,
    component: Cell<Option<ObjectId>>,
    listeners: Rc<ModelListeners>,
    allocated: RefCell<Option<DelegateMade>>,
    made: RefCell<Option<DelegateMade>>,
    gone: RefCell<Option<DelegateGone>>,
}

impl std::fmt::Debug for DelegateRows {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DelegateRows")
            .field("rows", &self.row_count())
            .field("component", &self.component.get())
            .finish()
    }
}

impl Drop for DelegateRows {
    fn drop(&mut self) {
        if let Some((m, id)) = self.model.get_mut().take() {
            m.unsubscribe(id);
        }
    }
}

impl DelegateRows {
    /// The model the rows are read from.
    pub fn model(&self) -> Option<Rc<dyn ItemModel>> {
        self.model.borrow().as_ref().map(|m| m.0.clone())
    }

    pub fn set_model(&self, engine: &Engine, model: Option<Rc<dyn ItemModel>>) {
        let subscribed = model.map(|m| {
            let listeners = Rc::downgrade(&self.listeners);
            let id = m.subscribe(Rc::new(move |e: &Engine, c: &ModelChange| {
                if let Some(l) = listeners.upgrade() {
                    l.emit(e, c);
                }
            }));
            (m, id)
        });
        let old = self.model.replace(subscribed);
        if let Some((m, id)) = old {
            m.unsubscribe(id);
        }
        self.listeners.emit(engine, &ModelChange::ModelReset);
    }

    pub fn component(&self) -> Option<ObjectId> {
        self.component.get()
    }

    pub fn set_component(&self, engine: &Engine, component: Option<ObjectId>) {
        if self.component.replace(component) != component {
            self.listeners.emit(engine, &ModelChange::ModelReset);
        }
    }

    /// What is told of every delegate a view makes of the component, and
    /// of every one that goes.
    pub fn on_delegates(&self, made: Option<DelegateMade>, gone: Option<DelegateGone>) {
        *self.made.borrow_mut() = made;
        *self.gone.borrow_mut() = gone;
    }

    /// What is told of a delegate before its bindings run: early enough
    /// for what the delegate itself asks about its place.
    pub fn on_allocated(&self, allocated: Option<DelegateMade>) {
        *self.allocated.borrow_mut() = allocated;
    }
}

impl ItemModel for DelegateRows {
    fn row_count(&self) -> usize {
        self.model().map_or(0, |m| m.row_count())
    }

    fn role_names(&self) -> Vec<Atom> {
        self.model().map_or_else(Vec::new, |m| m.role_names())
    }

    fn data(&self, engine: &Engine, row: usize, role: &str) -> Value {
        match self.model() {
            Some(m) => m.data(engine, row, role),
            None => Value::Undefined,
        }
    }

    fn set_data(&self, engine: &Engine, row: usize, role: &str, value: &Value) -> bool {
        self.model()
            .is_some_and(|m| m.set_data(engine, row, role, value))
    }

    fn row_key(&self, row: usize) -> RowKey {
        self.model().map_or(row as RowKey, |m| m.row_key(row))
    }

    fn model_data(&self, engine: &Engine, row: usize) -> Option<Value> {
        self.model()?.model_data(engine, row)
    }

    fn row_object(&self, row: usize) -> Option<ObjectId> {
        self.model()?.row_object(row)
    }

    fn subscribe(&self, listener: ModelListener) -> u64 {
        self.listeners.add(listener)
    }

    fn unsubscribe(&self, id: u64) {
        self.listeners.remove(id);
    }

    fn delegate_source(&self) -> DelegateSource {
        DelegateSource::Component(self.component.get())
    }

    fn delegate_allocated(&self, engine: &Engine, row: usize, object: ObjectId) {
        let allocated = self.allocated.borrow().clone();
        if let Some(allocated) = allocated {
            allocated(engine, row, object);
        }
    }

    fn delegate_made(&self, engine: &Engine, row: usize, object: ObjectId) {
        let made = self.made.borrow().clone();
        if let Some(made) = made {
            made(engine, row, object);
        }
    }

    fn delegate_gone(&self, engine: &Engine, object: ObjectId) {
        let gone = self.gone.borrow().clone();
        if let Some(gone) = gone {
            gone(engine, object);
        }
    }
}

/// The native part of a row object: it has no properties of its own, so
/// a role may have any name (`objectName` too).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQml.Models", name = "ModelRow")]
pub(crate) struct ModelRow {}

/// The type of a row object, and where its properties are.
#[derive(Debug)]
pub(crate) struct RowShape {
    pub(crate) meta: MetaRef,
    pub(crate) index: Option<PropIndex>,
    pub(crate) model: Option<PropIndex>,
    pub(crate) model_data: Option<PropIndex>,
    /// One property per role, in role order. A role named like one of
    /// the three above has none.
    pub(crate) roles: Vec<(Atom, PropIndex)>,
}

impl RowShape {
    pub(crate) fn role(&self, name: &str) -> Option<PropIndex> {
        self.roles.iter().find(|(n, _)| n == name).map(|(_, p)| *p)
    }

    fn of(meta: MetaRef, roles: &[Atom], kind: RowKind) -> Rc<RowShape> {
        let own = |name: &str| match kind {
            RowKind::Delegate => meta.property_index(name),
            RowKind::Element => None,
        };
        Rc::new(RowShape {
            index: own("index"),
            model: own("model"),
            model_data: own("modelData"),
            roles: roles
                .iter()
                .filter(|r| kind == RowKind::Element || !RESERVED.contains(&r.as_str()))
                .filter_map(|r| Some((r.clone(), meta.property_index(r)?)))
                .collect(),
            meta,
        })
    }
}

/// A row object made by [`Engine::make_row`].
#[derive(Clone, Debug)]
pub(crate) struct RowObject {
    pub(crate) id: ObjectId,
    pub(crate) shape: Rc<RowShape>,
    /// Its role properties are aliases of the row's object: there is
    /// nothing to keep up to date.
    pub(crate) aliased: bool,
}

/// What a row object is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RowKind {
    /// The row a delegate sees: `index`, `model`, `modelData` and roles.
    Delegate,
    /// What `ListModel.get()` returns: roles only.
    Element,
}

/// The names a delegate's row object has whatever the model.
const RESERVED: [&str; 3] = ["index", "model", "modelData"];

thread_local! {
    /// Row types by role list. A type derives from the one for the list
    /// without its last role, so when a model gains a role the row
    /// objects it has can take the wider type in place.
    static ROW_METAS: RefCell<HashMap<(RowKind, Vec<Atom>), MetaRef>> =
        RefCell::new(HashMap::new());
    /// Row types of object rows, by the address of the object's type. The
    /// type is kept so the address stays its.
    static OBJECT_ROW_METAS: RefCell<HashMap<usize, (MetaRef, Rc<RowShape>)>> =
        RefCell::new(HashMap::new());
}

fn row_meta(kind: RowKind, roles: &[Atom]) -> MetaRef {
    if let Some(m) = ROW_METAS.with(|c| c.borrow().get(&(kind, roles.to_vec())).cloned()) {
        return m;
    }
    let var =
        |b: MetaBuilder, name: &str| b.property(name, TypeTag::Var, PropertyFlags::empty(), true);
    let meta = match roles.split_last() {
        None => {
            let b = MetaBuilder::new("ModelRow")
                .base(MetaRef::Static(<ModelRow as QmlType>::static_meta()));
            match kind {
                RowKind::Delegate => RESERVED.iter().fold(b, |b, n| var(b, n)).build_shared(),
                RowKind::Element => b.build_shared(),
            }
        }
        Some((last, rest)) => {
            let base = row_meta(kind, rest);
            let b = MetaBuilder::new("ModelRow").base(base.clone());
            let taken = kind == RowKind::Delegate && RESERVED.contains(&last.as_str());
            if taken || base.property_index(last).is_some() {
                b.build_shared()
            } else {
                var(b, last).build_shared()
            }
        }
    };
    ROW_METAS.with(|c| {
        c.borrow_mut().insert((kind, roles.to_vec()), meta.clone());
    });
    meta
}

/// The row type for a model with `roles`.
pub(crate) fn row_shape(kind: RowKind, roles: &[Atom]) -> Rc<RowShape> {
    RowShape::of(row_meta(kind, roles), roles, kind)
}

/// The row type for a row that is an object of type `source`: the three
/// delegate names, and an alias for each of the object's properties.
fn object_row_shape(source: &MetaRef) -> Rc<RowShape> {
    let key = std::ptr::from_ref::<crate::meta::MetaObject>(source) as usize;
    if let Some(s) = OBJECT_ROW_METAS.with(|c| c.borrow().get(&key).map(|(_, s)| s.clone())) {
        return s;
    }
    let names: Vec<Atom> = source
        .properties
        .iter()
        .map(|p| p.name.clone())
        .filter(|n| !RESERVED.contains(&n.as_str()))
        .collect();
    let mut b = MetaBuilder::new("ModelRow").base(row_meta(RowKind::Delegate, &[]));
    for n in &names {
        b = b.property(n, TypeTag::Var, PropertyFlags::ALIAS, false);
    }
    let shape = RowShape::of(b.build_shared(), &names, RowKind::Delegate);
    OBJECT_ROW_METAS.with(|c| {
        c.borrow_mut().insert(key, (source.clone(), shape.clone()));
    });
    shape
}

impl Engine {
    /// The model `v` is to a view: what `model: v` means.
    ///
    /// - `null`, `undefined`: no rows;
    /// - a number: that many rows, each with its number as `modelData`
    ///   (a negative one is none, with Qt's warning written in the name of
    ///   `view`);
    /// - a list of objects, or one object: a row per object, whose
    ///   properties are the roles;
    /// - any other list, or a string (a row per character): the rows'
    ///   `modelData`;
    /// - a `ListModel`, or an object given a model with
    ///   [`Engine::set_object_model`]: that model.
    pub fn model_from_value(&self, v: &Value, view: Option<ObjectId>) -> Rc<dyn ItemModel> {
        let count = |n: f64| -> Rc<dyn ItemModel> {
            if n < 0.0 {
                let at = view.and_then(|o| self.location_of(o)).unwrap_or_default();
                let ty = view.and_then(|o| self.meta_of(o)).and_then(|m| {
                    m.chain()
                        .find(|m| m.module.is_some())
                        .map(|m| m.name.clone())
                });
                let ty = ty.map(|t| format!("QML {t}: ")).unwrap_or_default();
                self.warn(format!(
                    "{at}{ty}Model size of {} is less than 0",
                    crate::number::number_to_string(n)
                ));
            }
            // Qt rounds: `model: 2.7` is three rows.
            let n = if n.is_finite() {
                n.round().max(0.0)
            } else {
                0.0
            };
            Rc::new(CountModel(n as usize))
        };
        match v {
            Value::Undefined | Value::Null => Rc::new(CountModel(0)),
            Value::Bool(b) => Rc::new(CountModel(usize::from(*b))),
            Value::Int(n) => count(f64::from(*n)),
            Value::Real(n) => count(*n),
            Value::String(s) => Rc::new(ValuesModel(
                s.chars().map(|c| Value::string(c.to_string())).collect(),
            )),
            Value::List(l) => {
                let objects: Option<Vec<ObjectId>> = l.iter().map(Value::as_object).collect();
                match objects {
                    Some(o) if !o.is_empty() => Rc::new(ObjectsModel(o)),
                    _ => Rc::new(ValuesModel(l.to_vec())),
                }
            }
            Value::Object(o) => match self.object_model(*o) {
                Some(m) => m,
                // Nothing yet: what the object is to a view is known once
                // it is complete, and a view that tracked it
                // ([`Engine::track_model`]) resolves it again then.
                None if self.completes_later(*o) => Rc::new(CountModel(0)),
                None if self.is_alive(*o) => Rc::new(ObjectsModel(vec![*o])),
                None => Rc::new(CountModel(0)),
            },
            other => Rc::new(ValuesModel(vec![other.clone()])),
        }
    }

    /// Makes `obj` a model: `model: obj` then shows `model`'s rows rather
    /// than `obj` as one row. For model types written in Rust.
    pub fn set_object_model(
        &self,
        obj: ObjectId,
        model: Option<Rc<dyn ItemModel>>,
    ) -> Result<(), EngineError> {
        let mut store = self.inner.store.borrow_mut();
        let h = store
            .objects
            .get_mut(obj)
            .ok_or_else(|| EngineError::dead(obj))?;
        h.ext_mut().model = model;
        drop(store);
        self.model_changed(obj)?;
        Ok(())
    }

    /// The number of times readers of [`Engine::track_model`] for `obj`
    /// were told, read untracked.
    pub(crate) fn model_generation(&self, obj: ObjectId) -> Option<Value> {
        let node = {
            let store = self.inner.store.borrow();
            store
                .objects
                .get(obj)
                .and_then(|h| h.ext.as_ref()?.model_node)
        }?;
        let g = &self.inner.graph;
        g.untrack(|| g.try_get(node))
    }

    /// Re-runs the readers of [`Engine::track_model`] for `obj`.
    pub(crate) fn model_changed(&self, obj: ObjectId) -> Result<(), EngineError> {
        let node = {
            let store = self.inner.store.borrow();
            store
                .objects
                .get(obj)
                .and_then(|h| h.ext.as_ref()?.model_node)
        };
        if let Some(n) = node {
            let g = &self.inner.graph;
            let next = match g.untrack(|| g.try_get(n)) {
                Some(Value::Int(i)) => i.wrapping_add(1),
                _ => 1,
            };
            g.write(n, Value::Int(next), WriteKind::Initial)?;
        }
        Ok(())
    }

    /// Whether `obj` has native completion hooks still to run, in which
    /// it may make itself a model.
    fn completes_later(&self, obj: ObjectId) -> bool {
        let store = self.inner.store.borrow();
        store.objects.get(obj).is_some_and(|h| {
            !h.flags.contains(ObjectFlags::COMPLETED)
                && h.meta.chain().any(|m| m.on_complete.is_some())
        })
    }

    /// Makes the running binding or effect depend on what `model` is as
    /// a model: when it names an object that later becomes one
    /// ([`Engine::set_object_model`]), the reader runs again. A view
    /// calls it with its `model` value before it resolves the value
    /// (untracked) with [`Engine::model_from_value`], so the result does
    /// not depend on whether the view or the model completed first.
    ///
    /// The result counts the times the object's model was set (0 when
    /// `model` is no object or nothing tracks), for a reader that skips
    /// work when its `model` value is unchanged.
    pub fn track_model(&self, model: &Value) -> u64 {
        let Value::Object(obj) = model else { return 0 };
        if self.inner.tracking.get() == 0 {
            return 0;
        }
        let existing = {
            let store = self.inner.store.borrow();
            let Some(h) = store.objects.get(*obj) else {
                return 0;
            };
            h.ext.as_ref().and_then(|x| x.model_node)
        };
        let n = match existing {
            Some(n) => n,
            None => {
                let Ok(scope) = self.ensure_scope(*obj) else {
                    return 0;
                };
                let Ok(n) = self.inner.graph.property(scope, Value::Int(0)) else {
                    return 0;
                };
                if let Some(h) = self.inner.store.borrow_mut().objects.get_mut(*obj) {
                    h.ext_mut().model_node = Some(n);
                }
                n
            }
        };
        match self.inner.graph.try_get(n) {
            Some(Value::Int(i)) => i as u64,
            _ => 0,
        }
    }

    /// Makes `obj` the model `model`: `model: obj` in a view, a
    /// `Repeater` or an `Instantiator` shows its rows. The result is what
    /// the model reports its changes with.
    pub fn set_list_model(
        &self,
        obj: ObjectId,
        model: Rc<dyn QmlListModel>,
    ) -> Result<ModelSignals, EngineError> {
        let (model, signals) = list_model(model);
        self.set_object_model(obj, Some(model))?;
        Ok(signals)
    }

    /// The model `obj` is, when it is one.
    pub fn object_model(&self, obj: ObjectId) -> Option<Rc<dyn ItemModel>> {
        if let Some(state) = self.list_model_state(obj) {
            return Some(state);
        }
        if let Some(state) = self.sort_filter_state(obj) {
            return Some(state);
        }
        if let Some(state) = self.tree_model_state(obj) {
            return Some(state);
        }
        let store = self.inner.store.borrow();
        store.objects.get(obj)?.ext.as_ref()?.model.clone()
    }

    /// Sends every write from QML or JS to a property of `obj` through
    /// `hook` first; true from it means the write is done.
    pub(crate) fn set_write_hook(&self, obj: ObjectId, hook: Option<WriteHook>) {
        let mut store = self.inner.store.borrow_mut();
        if let Some(h) = store.objects.get_mut(obj) {
            h.ext_mut().write_hook = hook;
        }
    }

    /// Gives `obj` the type `meta`, which must derive from the type it
    /// has: the properties it had keep their places and values, the new
    /// ones start at their defaults.
    pub(crate) fn extend_meta(&self, obj: ObjectId, meta: MetaRef) -> Result<(), EngineError> {
        {
            let mut store = self.inner.store.borrow_mut();
            let h = store
                .objects
                .get_mut(obj)
                .ok_or_else(|| EngineError::dead(obj))?;
            if MetaRef::ptr_eq(&h.meta, &meta) {
                return Ok(());
            }
            if !meta.inherits(&h.meta) {
                return Err(EngineError::WrongType {
                    ty: meta.name.to_string(),
                    expected: h.meta.name.to_string(),
                });
            }
            let mut dynamic = std::mem::take(&mut h.dynamic).into_vec();
            let had = dynamic.len();
            dynamic.resize(meta.dynamic_slots as usize, Value::Undefined);
            for p in &meta.properties {
                if let PropertyAccess::Dynamic { slot } = p.access
                    && slot as usize >= had
                {
                    dynamic[slot as usize] = p.ty.default_value();
                }
            }
            h.dynamic = dynamic.into_boxed_slice();
            if let Some(nodes) = h.nodes.take() {
                let mut nodes = nodes.into_vec();
                nodes.resize(meta.properties.len(), None);
                h.nodes = Some(nodes.into_boxed_slice());
            }
            h.meta = meta;
        }
        // The JS wrapper was made for the old type and does not know the
        // new names; the next use makes one that does. Only the wrapper
        // goes: connections made from JS stay.
        let js = self.inner.js.borrow().clone();
        if let Some(js) = js {
            js.host.drop_wrapper(crate::js::obj_to_u64(obj));
        }
        Ok(())
    }

    /// Makes the row object a delegate sees row `row` of `model` through,
    /// owned by `owner`. `shape` is the model's; a row that is an object
    /// gets the shape of that object instead.
    pub(crate) fn make_row(
        &self,
        model: &Rc<dyn ItemModel>,
        shape: &Rc<RowShape>,
        row: usize,
        owner: Option<ObjectId>,
    ) -> Result<RowObject, EngineError> {
        let source = model.row_object(row).filter(|o| self.is_alive(*o));
        let shape = match source.and_then(|o| self.meta_of(o)) {
            Some(m) => object_row_shape(&m),
            None => shape.clone(),
        };
        let data = self.create_with(shape.meta.clone(), Box::new(ModelRow::default()), owner)?;
        let set = |p: Option<PropIndex>, v: Value| {
            if let Some(p) = p {
                let _ = self.write_internal(data, p, v, WriteKind::Initial);
            }
        };
        set(shape.index, Value::Int(row as i32));
        set(shape.model, Value::Object(data));
        set(
            shape.model_data,
            model.model_data(self, row).unwrap_or_default(),
        );
        match source {
            Some(o) => {
                let from = self.meta_of(o);
                for (name, p) in &shape.roles {
                    if let Some(sp) = from.as_ref().and_then(|m| m.property_index(name)) {
                        let _ = self.set_alias(data, *p, o, Some(sp));
                    }
                }
            }
            None => {
                for (name, p) in &shape.roles {
                    set(Some(*p), model.data(self, row, name));
                }
                self.set_write_hook(data, Some(row_hook(model.clone(), shape.clone(), data)));
            }
        }
        Ok(RowObject {
            id: data,
            shape,
            aliased: source.is_some(),
        })
    }
}

impl Engine {
    /// The row a row object showed left its model: its roles and its
    /// `modelData` read `undefined` from here on.
    ///
    /// Nothing is told that they changed. Measured on Qt 6.11: a binding
    /// in the delegate that read a role keeps what it read, and no change
    /// handler runs; only a new read finds the role gone.
    ///
    /// `delegate` is what was made for the row. Its bindings that have not
    /// run yet run first, while the roles are there: in Qt they ran when
    /// the delegate was made, and what they read then is what they hold.
    pub(crate) fn empty_row(&self, row: &RowObject, delegate: Option<ObjectId>) {
        // The roles of a row that is an object are the properties of an
        // object that is still there. A model of plain values (a number,
        // an array) has no roles, and its delegates keep `modelData`.
        if row.aliased || row.shape.roles.is_empty() {
            return;
        }
        let g = &self.inner.graph;
        if let Some(delegate) = delegate {
            let due: Vec<NodeId> = {
                let store = self.inner.store.borrow();
                let mut due = Vec::new();
                let mut stack = vec![delegate];
                while let Some(id) = stack.pop() {
                    let Some(h) = store.objects.get(id) else {
                        continue;
                    };
                    stack.extend(h.children.iter().copied());
                    let nodes = h.nodes.iter().flat_map(|n| n.iter().flatten());
                    due.extend(nodes.filter(|n| g.state(**n) != Some(NodeState::Clean)));
                }
                due
            };
            for n in due {
                if g.is_bound(n) {
                    let _ = self.untrack(|| g.try_get(n));
                }
            }
        }
        let roles = row.shape.roles.iter().map(|(_, p)| *p);
        for prop in roles.chain(row.shape.model_data) {
            let node = {
                let store = self.inner.store.borrow();
                let Some(h) = store.objects.get(row.id) else {
                    return;
                };
                h.node(prop)
            };
            let quiet = match node {
                // Nobody has read it: the object's copy is the only one.
                None => true,
                Some(n) => {
                    // A binding that an earlier change left to run again
                    // runs now, while the role is there: in Qt it ran
                    // when that change came, and holds that result.
                    for reader in g.observers(n) {
                        if g.kind(reader) == Some(NodeKind::Property)
                            && g.state(reader) != Some(NodeState::Clean)
                        {
                            let _ = self.untrack(|| g.try_get(reader));
                        }
                    }
                    g.replace_quietly(n, Value::Undefined).unwrap_or(false)
                }
            };
            if quiet {
                self.store_value(row.id, prop, &Value::Undefined);
            } else {
                // Something sits on the property (nothing of the engine's
                // does): the usual write decides what becomes of it.
                let _ = self.write_internal(row.id, prop, Value::Undefined, WriteKind::Initial);
            }
        }
    }
}

/// A write to a row object from QML or JS (`model.cost = 5`): it goes to
/// the model, whose change notification brings the value back.
pub(crate) fn row_hook(model: Rc<dyn ItemModel>, shape: Rc<RowShape>, data: ObjectId) -> WriteHook {
    Rc::new(move |e: &Engine, prop: PropIndex, v: &Value| {
        if Some(prop) == shape.index || Some(prop) == shape.model {
            // Not the delegate's to change.
            return true;
        }
        let row = shape
            .index
            .and_then(|p| e.untrack(|| e.try_read(data, p)).ok())
            .and_then(|v| v.as_number());
        let Some(row) = row.filter(|r| *r >= 0.0).map(|r| r as usize) else {
            return true;
        };
        if Some(prop) == shape.model_data {
            // A model of plain values keeps the write in the delegate, as
            // Qt does; a one-role model's `modelData` is that role.
            return match model.role_names().as_slice() {
                [role] => {
                    model.set_data(e, row, role, v);
                    true
                }
                _ => false,
            };
        }
        match shape.roles.iter().find(|(_, p)| *p == prop) {
            Some((name, _)) => {
                model.set_data(e, row, name, v);
                true
            }
            None => false,
        }
    })
}
