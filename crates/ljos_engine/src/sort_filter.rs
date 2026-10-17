//! `SortFilterProxyModel` and its sorter and filter types
//! (QtQml.Models, since Qt 6.10; the surface here is Qt 6.11's, where
//! the source is set with `model` — Qt 6.12's documentation calls the
//! same property `sourceModel`, and both names work here and keep each
//! other up to date).
//!
//! The proxy is an [`ItemModel`] over the source's rows: it keeps the
//! order its rows appear in (a list of source row keys), built by
//! refusing the rows its filters refuse and sorting what is left with
//! its sorters. A change in the source or in a sorter or filter is
//! turned into the row insertions, removals, moves and data changes
//! that take the old order to the new one, so a view keeps the
//! delegates of the rows that stay. Two changes reset instead because
//! Qt makes the delegates again (seen on qml6 of Qt 6.11.2): the
//! source resetting, and the source moving rows.
//!
//! Not built: `FunctionFilter`/`FunctionSorter` (qml6 of Qt 6.11.2
//! dumps core on the documented example, so there is no oracle for
//! them), sorting and filtering by column of a table model (rows here
//! have roles, and the one column is every role), and
//! `dynamicSortFilter: false` holding the order until `invalidate`
//! (the property is read and written; the proxy stays dynamic). The
//! scenarios for those are `@backlog` in
//! `features/models/sort-filter-proxy-model.feature`.

use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::atom::Atom;
use crate::builtins::QtObject;
use crate::cx::Cx;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::ObjectId;
use crate::list::QmlList;
use crate::model::{ItemModel, ModelChange, ModelListener, ModelListeners, RowKey};
use crate::value::Value;
use crate::{QmlObject, qml_impl};

/// The role a sorter or filter works on when none is named (Qt's
/// display role, which a model of roles has only if it says so).
fn display_role() -> Atom {
    Atom::intern("display")
}

/// What every filter carries. Abstract in Qt: it cannot be made in
/// QML, only the types that extend it.
#[derive(QmlObject)]
#[qml(module = "QtQml.Models", name = "FilterBase", extends = QtObject, uncreatable)]
pub struct FilterBase {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub enabled: bool,
    #[qml(notify)]
    pub inverted: bool,
    /// The column the filter works on; -1 is every column. Kept for
    /// the table models Ljós does not have yet: a list's rows are
    /// filtered by role, whatever this says.
    #[qml(notify)]
    pub column: i32,
}

impl Default for FilterBase {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            enabled: true,
            inverted: false,
            column: -1,
        }
    }
}

impl std::fmt::Debug for FilterBase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilterBase")
            .field("enabled", &self.enabled)
            .field("inverted", &self.inverted)
            .field("column", &self.column)
            .finish_non_exhaustive()
    }
}

#[qml_impl]
impl FilterBase {
    #[qml(signal, name = "invalidateModel")]
    fn invalidate_model();

    #[qml(signal, name = "invalidateCache")]
    fn invalidate_cache(filter: Option<ObjectId>);

    /// Asks the proxy this filter belongs to to look at its rows
    /// again. (Qt takes an optional `updateCache`; there is no cache
    /// here separate from the order itself.)
    #[qml(invokable)]
    fn invalidate(cx: &Cx, _update_cache: Value) -> Result<(), EngineError> {
        Self::emit_invalidate_model(cx)
    }
}

/// A filter that works on one role. Abstract in Qt.
#[derive(QmlObject)]
#[qml(module = "QtQml.Models", name = "RoleFilter", extends = FilterBase, uncreatable)]
pub struct RoleFilter {
    #[qml(base)]
    pub base: FilterBase,
    #[qml(notify, rename = "roleName")]
    pub role_name: Atom,
}

impl Default for RoleFilter {
    fn default() -> Self {
        Self {
            base: FilterBase::default(),
            role_name: display_role(),
        }
    }
}

impl std::fmt::Debug for RoleFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoleFilter")
            .field("role_name", &self.role_name)
            .finish_non_exhaustive()
    }
}

/// `ValueFilter`: a row appears when its role holds `value`. With no
/// value given no row matches, as in Qt (the role is compared with an
/// invalid variant).
#[derive(QmlObject)]
#[qml(module = "QtQml.Models", name = "ValueFilter", extends = RoleFilter)]
pub struct ValueFilter {
    #[qml(base)]
    pub base: RoleFilter,
    /// Qt gives this property a reset: assigning `undefined` takes
    /// it back to having no value.
    #[qml(notify, reset)]
    pub value: Value,
}

impl Default for ValueFilter {
    fn default() -> Self {
        Self {
            base: RoleFilter::default(),
            value: Value::Undefined,
        }
    }
}

impl std::fmt::Debug for ValueFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValueFilter")
            .field("value", &self.value)
            .finish_non_exhaustive()
    }
}

/// What every sorter carries. Abstract in Qt (named `Sorter` in the
/// Qt 6.12 documentation, `SorterBase` in Qt 6.11's type table).
#[derive(QmlObject)]
#[qml(module = "QtQml.Models", name = "SorterBase", extends = QtObject, uncreatable)]
pub struct SorterBase {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub enabled: bool,
    /// `Qt::AscendingOrder` (0) or `Qt::DescendingOrder` (1). The enum
    /// is Qt's `SortOrder` (`crates/ljos_engine/src/qt_enums.rs`).
    #[qml(notify, rename = "sortOrder", enumeration = "Qt.SortOrder")]
    pub sort_order: i32,
    /// The lower, the earlier the sorter decides. Qt 6.11.2 defaults
    /// this to the greatest `int` (read on qml6), not to the -1 the
    /// Qt 6.12 documentation says.
    #[qml(notify)]
    pub priority: i32,
    /// The column the sorter works on; kept, as a filter's is.
    #[qml(notify)]
    pub column: i32,
}

impl Default for SorterBase {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            enabled: true,
            sort_order: 0,
            priority: i32::MAX,
            column: 0,
        }
    }
}

impl std::fmt::Debug for SorterBase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SorterBase")
            .field("enabled", &self.enabled)
            .field("sort_order", &self.sort_order)
            .field("priority", &self.priority)
            .field("column", &self.column)
            .finish_non_exhaustive()
    }
}

#[qml_impl]
impl SorterBase {
    #[qml(signal, name = "invalidateModel")]
    fn invalidate_model();

    #[qml(signal, name = "invalidateCache")]
    fn invalidate_cache(filter: Option<ObjectId>);

    #[qml(invokable)]
    fn invalidate(cx: &Cx, _update_cache: Value) -> Result<(), EngineError> {
        Self::emit_invalidate_model(cx)
    }
}

/// `RoleSorter`: orders the rows by one role — numbers by value, text
/// by its characters, `true` after `false`, as Qt compares variants.
#[derive(QmlObject)]
#[qml(module = "QtQml.Models", name = "RoleSorter", extends = SorterBase)]
pub struct RoleSorter {
    #[qml(base)]
    pub base: SorterBase,
    #[qml(notify, rename = "roleName")]
    pub role_name: Atom,
}

impl Default for RoleSorter {
    fn default() -> Self {
        Self {
            base: SorterBase::default(),
            role_name: display_role(),
        }
    }
}

impl std::fmt::Debug for RoleSorter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoleSorter")
            .field("role_name", &self.role_name)
            .finish_non_exhaustive()
    }
}

/// `StringSorter`: orders text as text. Only the case sensitivity is
/// honoured so far; `locale`, `numericMode` and `ignorePunctuation`
/// are read and written but do not change the order (collation is
/// what `ljos_i18n` will bring).
#[derive(QmlObject)]
#[qml(module = "QtQml.Models", name = "StringSorter", extends = RoleSorter)]
pub struct StringSorter {
    #[qml(base)]
    pub base: RoleSorter,
    /// `Qt::CaseInsensitive` (0) or `Qt::CaseSensitive` (1).
    #[qml(notify, rename = "caseSensitivity", enumeration = "Qt.CaseSensitivity")]
    pub case_sensitivity: i32,
    #[qml(notify, rename = "ignorePunctuation")]
    pub ignore_punctuation: bool,
    #[qml(notify)]
    pub locale: Value,
    #[qml(notify, rename = "numericMode")]
    pub numeric_mode: bool,
}

impl Default for StringSorter {
    fn default() -> Self {
        Self {
            base: RoleSorter::default(),
            case_sensitivity: 1,
            ignore_punctuation: false,
            locale: Value::Undefined,
            numeric_mode: false,
        }
    }
}

impl std::fmt::Debug for StringSorter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StringSorter")
            .field("case_sensitivity", &self.case_sensitivity)
            .finish_non_exhaustive()
    }
}

/// A sorter as the proxy reads it, in the order it decides.
struct SorterCfg {
    role: Atom,
    descending: bool,
    /// For a `StringSorter`: fold case before comparing.
    fold_case: bool,
}

/// A `ValueFilter` as the proxy reads it.
struct FilterCfg {
    role: Atom,
    value: Value,
    inverted: bool,
}

/// The rows of a `SortFilterProxyModel`, shared with the views that
/// show it: the source and the order its rows appear in.
struct ProxyState {
    me: Cell<Option<ObjectId>>,
    source: RefCell<Option<(Rc<dyn ItemModel>, u64)>>,
    /// The source value last set, so a new one that is the same model
    /// by another road is no change.
    source_value: RefCell<Value>,
    /// Proxy row to source row key, in proxy order.
    order: RefCell<Vec<RowKey>>,
    /// Source row key to source row, for the rows of `order`.
    source_row: RefCell<HashMap<RowKey, usize>>,
    primary: Cell<Option<ObjectId>>,
    /// Sorters and filters already watched, so an effect that runs
    /// again does not watch them twice.
    watched: RefCell<HashSet<ObjectId>>,
    listeners: ModelListeners,
}

impl ProxyState {
    fn new() -> Rc<ProxyState> {
        Rc::new(ProxyState {
            me: Cell::new(None),
            source: RefCell::new(None),
            source_value: RefCell::new(Value::Undefined),
            order: RefCell::new(Vec::new()),
            source_row: RefCell::new(HashMap::new()),
            primary: Cell::new(None),
            watched: RefCell::new(HashSet::new()),
            listeners: ModelListeners::default(),
        })
    }

    fn model(&self) -> Option<Rc<dyn ItemModel>> {
        self.source.borrow().as_ref().map(|s| s.0.clone())
    }

    /// The sorters and filters, read where they are. What is in the
    /// lists that is not one (a sorter type that is not built) does
    /// nothing.
    fn config(&self, engine: &Engine) -> (Vec<FilterCfg>, Vec<SorterCfg>) {
        let Some(me) = self.me.get() else {
            return (Vec::new(), Vec::new());
        };
        let Ok((filters, sorters)) = engine.untrack(|| {
            engine
                .with::<SortFilterProxyModel, _>(me, |p| (p.filters.objects(), p.sorters.objects()))
        }) else {
            return (Vec::new(), Vec::new());
        };
        let filters = filters
            .iter()
            .filter_map(|o| filter_cfg(engine, *o))
            .collect();
        let mut sorters: Vec<(i32, usize, ObjectId, SorterCfg)> = sorters
            .iter()
            .enumerate()
            .filter_map(|(i, o)| sorter_cfg(engine, *o).map(|(p, c)| (p, i, *o, c)))
            .collect();
        // The primary sorter decides first, then the lower priority,
        // then the order they are written in.
        let primary = self.primary.get();
        sorters.sort_by_key(|(p, i, o, _)| (Some(*o) != primary, *p, *i));
        (filters, sorters.into_iter().map(|(_, _, _, c)| c).collect())
    }

    /// The order the source's rows appear in under the sorter and
    /// filter configuration `engine` reads now.
    fn build(&self, engine: &Engine) -> (Vec<RowKey>, HashMap<RowKey, usize>) {
        let Some(source) = self.model() else {
            return (Vec::new(), HashMap::new());
        };
        let (filters, sorters) = self.config(engine);
        let mut rows: Vec<usize> = (0..source.row_count())
            .filter(|&r| {
                filters.iter().all(|f| {
                    let v = source.data(engine, r, &f.role);
                    v.strict_equals(&f.value) != f.inverted
                })
            })
            .collect();
        if !sorters.is_empty() {
            // Read each row's values once, not once per comparison.
            let values: HashMap<usize, Vec<Value>> = rows
                .iter()
                .map(|&r| {
                    (
                        r,
                        sorters
                            .iter()
                            .map(|s| source.data(engine, r, &s.role))
                            .collect(),
                    )
                })
                .collect();
            let flip_tail = sorters.first().is_some_and(|s| s.descending);
            rows.sort_by(|&a, &b| {
                for (i, s) in sorters.iter().enumerate() {
                    let c = compare(&values[&a][i], &values[&b][i], s.fold_case);
                    if c != Ordering::Equal {
                        return if s.descending { c.reverse() } else { c };
                    }
                }
                // Rows the sorters cannot tell apart keep their source
                // order, turned around when the deciding sorter runs
                // descending: a descending order in Qt is the
                // ascending one reversed, equal rows included (seen
                // on qml6 of Qt 6.11.2).
                if flip_tail { b.cmp(&a) } else { a.cmp(&b) }
            });
        }
        let order: Vec<RowKey> = rows.iter().map(|&r| source.row_key(r)).collect();
        let at: HashMap<RowKey, usize> = rows.iter().map(|&r| (source.row_key(r), r)).collect();
        (order, at)
    }

    fn set_order(&self, order: Vec<RowKey>, at: HashMap<RowKey, usize>) {
        *self.order.borrow_mut() = order;
        *self.source_row.borrow_mut() = at;
    }

    /// Builds the new order and tells the listeners what takes the
    /// old one to it: removals first (from the end, so the rows
    /// before a run keep their numbers while it is told), then
    /// moves, then insertions where they belong.
    fn refresh(&self, engine: &Engine) {
        let (new, at) = self.build(engine);
        let old = self.order.borrow().clone();
        *self.source_row.borrow_mut() = at;
        if old == new {
            return;
        }
        let new_set: HashSet<RowKey> = new.iter().copied().collect();
        let old_set: HashSet<RowKey> = old.iter().copied().collect();
        engine.batch(|| {
            let mut work = old;
            // The runs of `work` that are not in `new`, last first.
            let mut runs: Vec<(usize, usize)> = Vec::new();
            let mut i = 0;
            while i < work.len() {
                if new_set.contains(&work[i]) {
                    i += 1;
                    continue;
                }
                let first = i;
                while i < work.len() && !new_set.contains(&work[i]) {
                    i += 1;
                }
                runs.push((first, i - first));
            }
            for (first, count) in runs.into_iter().rev() {
                self.listeners
                    .emit(engine, &ModelChange::RowsAboutToBeRemoved { first, count });
                work.drain(first..first + count);
                *self.order.borrow_mut() = work.clone();
                self.listeners
                    .emit(engine, &ModelChange::RowsRemoved { first, count });
            }
            let common: Vec<RowKey> = new
                .iter()
                .copied()
                .filter(|k| old_set.contains(k))
                .collect();
            for i in 0..work.len() {
                if work[i] == common[i] {
                    continue;
                }
                let Some(j) = work[i + 1..].iter().position(|k| *k == common[i]) else {
                    continue;
                };
                let from = i + 1 + j;
                let key = work.remove(from);
                work.insert(i, key);
                *self.order.borrow_mut() = work.clone();
                self.listeners.emit(
                    engine,
                    &ModelChange::RowsMoved {
                        from,
                        to: i,
                        count: 1,
                    },
                );
            }
            for (at, key) in new.iter().enumerate() {
                if old_set.contains(key) {
                    continue;
                }
                self.listeners.emit(
                    engine,
                    &ModelChange::RowsAboutToBeInserted {
                        first: at,
                        count: 1,
                    },
                );
                work.insert(at.min(work.len()), *key);
                *self.order.borrow_mut() = work.clone();
                self.listeners.emit(
                    engine,
                    &ModelChange::RowsInserted {
                        first: at,
                        count: 1,
                    },
                );
            }
            *self.order.borrow_mut() = new;
        });
    }

    /// A change of the source, told in the proxy's rows.
    fn on_source_change(self: &Rc<Self>, engine: &Engine, change: &ModelChange) {
        match change {
            // The change itself follows; a view is told then.
            ModelChange::RowsAboutToBeInserted { .. }
            | ModelChange::RowsAboutToBeRemoved { .. } => {}
            // Qt makes a view's delegates again for both (seen on
            // qml6 of Qt 6.11.2: a source move through the proxy
            // recreates every delegate, read straight from the
            // source it keeps them).
            ModelChange::ModelReset | ModelChange::RowsMoved { .. } => {
                let (order, at) = self.build(engine);
                self.set_order(order, at);
                self.listeners.emit(engine, &ModelChange::ModelReset);
            }
            ModelChange::RolesChanged => {
                self.refresh(engine);
                self.listeners.emit(engine, &ModelChange::RolesChanged);
            }
            ModelChange::DataChanged {
                first,
                count,
                roles,
            } => {
                let was: HashSet<RowKey> = self.order.borrow().iter().copied().collect();
                let keys: Vec<RowKey> = match self.model() {
                    Some(m) => (*first..first + count).map(|r| m.row_key(r)).collect(),
                    None => Vec::new(),
                };
                self.refresh(engine);
                // A row that stayed is read again; one the refresh
                // inserted was made with the new values, and one it
                // removed is gone.
                let order = self.order.borrow().clone();
                for key in keys.iter().filter(|k| was.contains(k)) {
                    if let Some(at) = order.iter().position(|k| k == key) {
                        self.listeners.emit(
                            engine,
                            &ModelChange::DataChanged {
                                first: at,
                                count: 1,
                                roles: roles.clone(),
                            },
                        );
                    }
                }
            }
            _ => self.refresh(engine),
        }
    }

    /// The source the rows are read from. A model the engine knows
    /// (`ListModel`, another proxy, a model written in Rust); a plain
    /// object is none, as Qt takes only an item model.
    fn set_source(self: &Rc<Self>, engine: &Engine, value: &Value) {
        if self.source_value.borrow().change_equals(value) && self.source.borrow().is_some() {
            return;
        }
        *self.source_value.borrow_mut() = value.clone();
        let model = match value {
            Value::Object(o) if engine.is_alive(*o) => engine.object_model(*o),
            Value::Undefined | Value::Null | Value::Object(_) => None,
            other => Some(engine.model_from_value(other, self.me.get())),
        };
        let weak = Rc::downgrade(self);
        let subscribed = model.map(|m| {
            let id = m.subscribe(Rc::new(move |e: &Engine, c: &ModelChange| {
                if let Some(state) = weak.upgrade() {
                    state.on_source_change(e, c);
                }
            }));
            (m, id)
        });
        if let Some((m, id)) = self.source.replace(subscribed) {
            m.unsubscribe(id);
        }
        let (order, at) = self.build(engine);
        self.set_order(order, at);
        self.listeners.emit(engine, &ModelChange::ModelReset);
    }

    /// Watches the sorters and filters not watched yet, so a change
    /// of any of their properties is a change of the order.
    fn watch_helpers(self: &Rc<Self>, engine: &Engine, helpers: &[ObjectId], names: &[&str]) {
        for &o in helpers {
            if !engine.is_alive(o) || !self.watched.borrow_mut().insert(o) {
                continue;
            }
            for name in names {
                let Ok(prop) = engine.property_index(o, name) else {
                    continue;
                };
                let weak = Rc::downgrade(self);
                let _ = engine.watch(o, prop, move |e, _| {
                    if let Some(state) = weak.upgrade() {
                        state.refresh(e);
                    }
                });
            }
        }
    }
}

impl ItemModel for ProxyState {
    fn row_count(&self) -> usize {
        self.order.borrow().len()
    }

    fn role_names(&self) -> Vec<Atom> {
        self.model().map_or_else(Vec::new, |m| m.role_names())
    }

    fn data(&self, engine: &Engine, row: usize, role: &str) -> Value {
        let key = self.order.borrow().get(row).copied();
        let at = key.and_then(|k| self.source_row.borrow().get(&k).copied());
        match (self.model(), at) {
            (Some(m), Some(at)) => m.data(engine, at, role),
            _ => Value::Undefined,
        }
    }

    fn set_data(&self, engine: &Engine, row: usize, role: &str, value: &Value) -> bool {
        let key = self.order.borrow().get(row).copied();
        let at = key.and_then(|k| self.source_row.borrow().get(&k).copied());
        match (self.model(), at) {
            (Some(m), Some(at)) => m.set_data(engine, at, role, value),
            _ => false,
        }
    }

    fn row_key(&self, row: usize) -> RowKey {
        self.order.borrow().get(row).copied().unwrap_or(RowKey::MAX)
    }

    fn model_data(&self, engine: &Engine, row: usize) -> Option<Value> {
        let key = self.order.borrow().get(row).copied()?;
        let at = self.source_row.borrow().get(&key).copied()?;
        self.model()?.model_data(engine, at)
    }

    fn row_object(&self, row: usize) -> Option<ObjectId> {
        let key = self.order.borrow().get(row).copied()?;
        let at = self.source_row.borrow().get(&key).copied()?;
        self.model()?.row_object(at)
    }

    fn subscribe(&self, listener: ModelListener) -> u64 {
        self.listeners.add(listener)
    }

    fn unsubscribe(&self, id: u64) {
        self.listeners.remove(id);
    }

    fn delegate_source(&self) -> crate::model::DelegateSource {
        // The source's own say in its delegates (an `ObjectModel`'s
        // items, a `DelegateModel`'s component) goes through.
        self.model()
            .map_or(crate::model::DelegateSource::View, |m| m.delegate_source())
    }

    fn delegate_allocated(&self, engine: &Engine, row: usize, object: ObjectId) {
        if let (Some(m), Some(at)) = (self.model(), self.source_at(row)) {
            m.delegate_allocated(engine, at, object);
        }
    }

    fn delegate_made(&self, engine: &Engine, row: usize, object: ObjectId) {
        if let (Some(m), Some(at)) = (self.model(), self.source_at(row)) {
            m.delegate_made(engine, at, object);
        }
    }

    fn delegate_gone(&self, engine: &Engine, object: ObjectId) {
        if let Some(m) = self.model() {
            m.delegate_gone(engine, object);
        }
    }
}

impl ProxyState {
    fn source_at(&self, row: usize) -> Option<usize> {
        let key = self.order.borrow().get(row).copied()?;
        self.source_row.borrow().get(&key).copied()
    }
}

/// The `ValueFilter` `obj` as a configuration, or none when it is
/// disabled or not one.
fn filter_cfg(engine: &Engine, obj: ObjectId) -> Option<FilterCfg> {
    let f = engine
        .untrack(|| {
            engine.with::<ValueFilter, _>(obj, |f| {
                (
                    f.base.base.enabled,
                    f.base.base.inverted,
                    f.base.role_name.clone(),
                    f.value.clone(),
                )
            })
        })
        .ok()?;
    f.0.then_some(FilterCfg {
        role: f.2,
        value: f.3,
        inverted: f.1,
    })
}

/// The sorter `obj` and its priority, or none when it is disabled or
/// not a sorter that is built.
fn sorter_cfg(engine: &Engine, obj: ObjectId) -> Option<(i32, SorterCfg)> {
    if let Ok(s) = engine.untrack(|| {
        engine.with::<StringSorter, _>(obj, |s| {
            (
                s.base.base.enabled,
                s.base.base.sort_order,
                s.base.base.priority,
                s.base.role_name.clone(),
                s.case_sensitivity == 0,
            )
        })
    }) {
        return s.0.then_some((
            s.2,
            SorterCfg {
                role: s.3,
                descending: s.1 == 1,
                fold_case: s.4,
            },
        ));
    }
    let s = engine
        .untrack(|| {
            engine.with::<RoleSorter, _>(obj, |s| {
                (
                    s.base.enabled,
                    s.base.sort_order,
                    s.base.priority,
                    s.role_name.clone(),
                )
            })
        })
        .ok()?;
    s.0.then_some((
        s.2,
        SorterCfg {
            role: s.3,
            descending: s.1 == 1,
            fold_case: false,
        },
    ))
}

/// Two role values in sorting order: numbers by value (a `NaN` before
/// any other number, as Qt compares them), text by its characters,
/// `false` before `true`, and values of different kinds by kind —
/// a missing role first, then booleans, numbers, then text.
fn compare(a: &Value, b: &Value, fold_case: bool) -> Ordering {
    if a.strict_equals(b) {
        return Ordering::Equal;
    }
    let rank = |v: &Value| match v {
        Value::Undefined | Value::Null => 0,
        Value::Bool(_) => 1,
        v if v.as_number().is_some() => 2,
        _ => 3,
    };
    let (ra, rb) = (rank(a), rank(b));
    if ra != rb {
        return ra.cmp(&rb);
    }
    match (a, b) {
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        _ if a.as_number().is_some() => {
            let (x, y) = (a.to_number(), b.to_number());
            match (x.is_nan(), y.is_nan()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
            }
        }
        _ => {
            let (x, y) = (a.to_js_string(), b.to_js_string());
            if fold_case {
                x.to_lowercase().cmp(&y.to_lowercase())
            } else {
                x.cmp(&y)
            }
        }
    }
}

/// `SortFilterProxyModel`: the rows of its `model`, filtered by its
/// `filters` and sorted by its `sorters`.
///
/// There is no `count` property (qml6 reads `undefined`), as Qt's is
/// an item model and not a `ListModel`; a view's count is the count
/// to read. `rowCount()` is there, as on Qt's item models.
#[derive(QmlObject)]
#[qml(module = "QtQml.Models", name = "SortFilterProxyModel", extends = QtObject)]
pub struct SortFilterProxyModel {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub model: Value,
    /// The name Qt 6.12's documentation gives `model`; the two keep
    /// each other up to date.
    #[qml(notify, rename = "sourceModel")]
    pub source_model: Value,
    #[qml(notify, list, readonly)]
    pub filters: QmlList,
    #[qml(notify, list, readonly)]
    pub sorters: QmlList,
    /// Read and written; the proxy does not yet hold its order when
    /// this is false (see the module's notes).
    #[qml(notify, rename = "dynamicSortFilter")]
    pub dynamic_sort_filter: bool,
    /// Tree models only; kept, as a filter's `column` is.
    #[qml(notify, rename = "recursiveFiltering")]
    pub recursive_filtering: bool,
    #[qml(notify, rename = "autoAcceptChildRows")]
    pub auto_accept_child_rows: bool,
    #[qml(skip)]
    state: Rc<ProxyState>,
}

impl Default for SortFilterProxyModel {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            model: Value::Undefined,
            source_model: Value::Undefined,
            filters: QmlList::default(),
            sorters: QmlList::default(),
            dynamic_sort_filter: true,
            recursive_filtering: false,
            auto_accept_child_rows: false,
            state: ProxyState::new(),
        }
    }
}

impl std::fmt::Debug for SortFilterProxyModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SortFilterProxyModel")
            .field("rows", &self.state.row_count())
            .finish_non_exhaustive()
    }
}

impl Engine {
    /// The model the `SortFilterProxyModel` `obj` is. Asked by
    /// [`Engine::object_model`] as it asks for a `ListModel`'s state:
    /// a view completed before the proxy (completion runs last-made
    /// first) must already get the proxy's rows, empty until the
    /// proxy completes and sets its source, which it is then told of
    /// as a reset.
    pub(crate) fn sort_filter_state(&self, obj: ObjectId) -> Option<Rc<dyn ItemModel>> {
        state_of(self, obj).map(|s| s as Rc<dyn ItemModel>)
    }
}

/// The state of the `SortFilterProxyModel` `obj`.
fn state_of(engine: &Engine, obj: ObjectId) -> Option<Rc<ProxyState>> {
    let state = engine
        .with::<SortFilterProxyModel, _>(obj, |p| p.state.clone())
        .ok()?;
    state.me.set(Some(obj));
    Some(state)
}

const SORTER_PROPS: &[&str] = &[
    "enabled",
    "sortOrder",
    "priority",
    "column",
    "roleName",
    "caseSensitivity",
    "ignorePunctuation",
    "numericMode",
    "locale",
];
const FILTER_PROPS: &[&str] = &["enabled", "inverted", "column", "roleName", "value"];

#[qml_impl]
impl SortFilterProxyModel {
    #[qml(signal, name = "primarySorterChanged")]
    fn primary_sorter_changed();

    /// Looks at the filters and sorters against the source again.
    #[qml(invokable)]
    fn invalidate(cx: &Cx) {
        if let Some(state) = state_of(cx.engine, cx.this) {
            state.refresh(cx.engine);
        }
    }

    /// Looks at the sorters against the source again. (Nothing is
    /// cached separately from the order, so this is [`invalidate`].)
    #[qml(invokable, name = "invalidateSorter")]
    fn invalidate_sorter(cx: &Cx) {
        Self::invalidate(cx);
    }

    /// The sorter that decides first, ahead of every priority;
    /// `null` gives the say back to the priorities.
    #[qml(invokable, name = "setPrimarySorter")]
    fn set_primary_sorter(cx: &Cx, sorter: Value) -> Result<(), EngineError> {
        if let Some(state) = state_of(cx.engine, cx.this) {
            state.primary.set(sorter.as_object());
            state.refresh(cx.engine);
        }
        Self::emit_primary_sorter_changed(cx)
    }

    /// How many rows the proxy has, as on Qt's item models.
    #[qml(invokable, name = "rowCount")]
    fn row_count(cx: &Cx) -> i32 {
        state_of(cx.engine, cx.this).map_or(0, |s| s.row_count() as i32)
    }

    /// From completion on the object is a model, and it follows its
    /// `model`, its lists, and every sorter and filter in them.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let Some(state) = state_of(e, me) else {
            return Ok(());
        };
        e.set_object_model(me, Some(state.clone()))?;
        // `model` and `sourceModel` are the one property by two names.
        for (from, to) in [
            (
                SortFilterProxyModel::MODEL.index,
                SortFilterProxyModel::SOURCE_MODEL,
            ),
            (
                SortFilterProxyModel::SOURCE_MODEL.index,
                SortFilterProxyModel::MODEL,
            ),
        ] {
            e.watch(me, from, move |e, v| {
                let now = e.untrack(|| e.get(me, to)).unwrap_or_default();
                if !now.change_equals(v) {
                    let _ = e.set_internal(me, to, v.clone());
                }
            })?;
        }
        let shown = state.clone();
        e.effect_in(me, move |e| {
            let model = e.get(me, SortFilterProxyModel::MODEL).unwrap_or_default();
            let filters = e.get(me, SortFilterProxyModel::FILTERS).unwrap_or_default();
            let sorters = e.get(me, SortFilterProxyModel::SORTERS).unwrap_or_default();
            // A read for the tracking only: the values are read where
            // they are kept, in `config`.
            let _ = (&filters, &sorters);
            e.track_model(&model);
            e.untrack(|| {
                shown.set_source(e, &model);
                let (filters, sorters) = e
                    .with::<SortFilterProxyModel, _>(me, |p| {
                        (p.filters.objects(), p.sorters.objects())
                    })
                    .unwrap_or_default();
                shown.watch_helpers(e, &filters, FILTER_PROPS);
                shown.watch_helpers(e, &sorters, SORTER_PROPS);
                shown.refresh(e);
            });
        })?;
        Ok(())
    }
}
