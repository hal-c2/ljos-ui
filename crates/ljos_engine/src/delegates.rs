//! One delegate per model row, kept in step with the model: what
//! `Repeater` and `Instantiator` share (and what a non-virtualised view
//! is, ARCHITECTURE.md §8.4).
//!
//! [`Delegates`] listens to an [`ItemModel`] and creates, destroys and
//! reorders delegates for exactly the rows a change names: appending a
//! row makes one delegate and touches no other. Each delegate reads its
//! row through a row object (see [`crate::model`]):
//!
//! - a delegate without required properties gets the row object as its
//!   context object, so `index`, `model`, `modelData` and the role names
//!   are in scope;
//! - a delegate that declares required properties gets nothing in scope.
//!   Each required property named like something the row has is bound
//!   to it, as Qt does.
//!
//! The view says what it wants done with the objects through a
//! [`DelegateHost`].

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_runtime::{Lane, ProcId};

use crate::WriteKind;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{ContextId, ObjectId, PropIndex};
use crate::incubator::{CreationHandle, CreationJob, SharedCreation};
use crate::log::LogLevel;
use crate::meta::PropertyFlags;
use crate::model::{
    DelegateSource, ItemModel, ModelChange, ModelRow, RowKey, RowKind, RowObject, RowShape,
    row_hook, row_shape,
};
use crate::value::Value;

/// What a view does with its delegates. Called with nothing borrowed:
/// an implementation may call back into its [`Delegates`].
pub trait DelegateHost {
    /// The root of the delegate for row `index` exists and nothing in it
    /// has run yet: the place to give it a visual parent, so that
    /// `parent` reads right inside it. `before` is the delegate of the
    /// nearest later row that has one.
    fn allocated(&self, engine: &Engine, index: usize, object: ObjectId, before: Option<ObjectId>) {
        let _ = (engine, index, object, before);
    }

    /// The delegate for row `index` is complete.
    fn added(&self, engine: &Engine, index: usize, object: ObjectId);

    /// The delegate that was at `index` is about to be destroyed.
    fn removed(&self, engine: &Engine, index: usize, object: ObjectId);

    /// Rows changed places; [`Delegates::objects`] has the new order.
    fn moved(&self, engine: &Engine) {
        let _ = engine;
    }

    /// The number of rows changed.
    fn count_changed(&self, engine: &Engine, count: usize);
}

/// The order delegates go in when all of them go at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemovalOrder {
    FirstToLast,
    /// What Qt's `Repeater` does.
    LastToFirst,
}

struct Row {
    /// Finds the row again after user code ran, whatever moved.
    id: u64,
    data: Option<RowObject>,
    object: Option<ObjectId>,
    pending: Option<CreationHandle>,
}

#[derive(Default)]
struct State {
    model: Option<Rc<dyn ItemModel>>,
    subscription: u64,
    component: Option<ObjectId>,
    rows: Vec<Row>,
    next_id: u64,
    asynchronous: bool,
    created: u64,
    /// Where a row was last found. Rows are made one after the other, so
    /// the next one asked for is this one or its neighbour, and making n
    /// delegates does not look through n rows for each.
    near: Cell<usize>,
    /// What [`State::object_from`] last found.
    later: Cell<Option<Later>>,
}

/// No row in `from..at` has a delegate and the row at `at` has one, or
/// none from `from` on has when `at` is `None`.
#[derive(Clone, Copy)]
struct Later {
    from: usize,
    at: Option<usize>,
}

impl State {
    fn position(&self, id: u64) -> Option<usize> {
        let near = self.near.get();
        let is = |i: usize| self.rows.get(i).is_some_and(|r| r.id == id);
        let found = if is(near) {
            near
        } else if is(near + 1) {
            near + 1
        } else {
            self.rows.iter().position(|r| r.id == id)?
        };
        self.near.set(found);
        Some(found)
    }

    /// The delegate of the nearest row from `from` on that has one.
    ///
    /// While many rows are being made, every row after the one in hand is
    /// still empty, so the answer is remembered: looking again for each
    /// row would make n delegates cost n squared.
    fn object_from(&self, from: usize) -> Option<ObjectId> {
        if let Some(l) = self.later.get()
            && from >= l.from
            && l.at.is_none_or(|at| from <= at)
        {
            // Narrowed to what was asked, so that the row before `from`
            // getting its delegate does not make this stale.
            self.later.set(Some(Later { from, at: l.at }));
            return l.at.and_then(|at| self.rows.get(at)?.object);
        }
        let at = self
            .rows
            .get(from..)?
            .iter()
            .position(|r| r.object.is_some())
            .map(|i| from + i);
        self.later.set(Some(Later { from, at }));
        at.and_then(|at| self.rows[at].object)
    }

    /// The row at `index` has its delegate.
    fn set_object(&mut self, index: usize, object: ObjectId) {
        self.rows[index].object = Some(object);
        if let Some(l) = self.later.get()
            && index >= l.from
            && l.at.is_none_or(|at| index < at)
        {
            self.later.set(None);
        }
    }

    fn placeholders(&mut self, at: usize, count: usize) -> Vec<u64> {
        let ids: Vec<u64> = (0..count as u64).map(|i| self.next_id + i).collect();
        self.next_id += count as u64;
        let at = at.min(self.rows.len());
        self.later.set(None);
        self.rows.splice(
            at..at,
            ids.iter().map(|id| Row {
                id: *id,
                data: None,
                object: None,
                pending: None,
            }),
        );
        ids
    }
}

struct Shared {
    owner: ObjectId,
    host: Rc<dyn DelegateHost>,
    removal: RemovalOrder,
    state: RefCell<State>,
}

impl Drop for Shared {
    fn drop(&mut self) {
        let st = self.state.get_mut();
        if let Some(m) = st.model.take() {
            m.unsubscribe(st.subscription);
        }
    }
}

/// The delegates of one view. Cloning gives another handle to the same
/// set.
#[derive(Clone)]
pub struct Delegates {
    shared: Rc<Shared>,
}

impl std::fmt::Debug for Delegates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Delegates")
            .field("owner", &self.shared.owner)
            .field("count", &self.count())
            .finish()
    }
}

impl Delegates {
    /// Delegates owned by `owner` (the view object).
    pub fn new(owner: ObjectId, host: Rc<dyn DelegateHost>, removal: RemovalOrder) -> Self {
        Delegates {
            shared: Rc::new(Shared {
                owner,
                host,
                removal,
                state: RefCell::new(State::default()),
            }),
        }
    }

    /// Replaces every delegate: one of `component` (a `Component`
    /// object) per row of `model`. Without either there are none.
    pub fn set_source(
        &self,
        engine: &Engine,
        model: Option<Rc<dyn ItemModel>>,
        component: Option<ObjectId>,
    ) {
        let s = &self.shared;
        let model = model.filter(|_| component.is_some());
        engine.batch(|| {
            // Measured on Qt 6.11: a new delegate takes the old objects
            // away as clearing the model would; a new model leaves them
            // their index and goes in the view's order.
            if s.state.borrow().component == component {
                s.remove_all(engine);
            } else {
                s.remove_rows(engine, 0, usize::MAX);
            }
            let old = {
                let mut st = s.state.borrow_mut();
                st.component = component;
                st.model.take().map(|m| (m, st.subscription))
            };
            if let Some((m, id)) = old {
                m.unsubscribe(id);
            }
            if let Some(m) = &model {
                let weak = Rc::downgrade(s);
                let id = m.subscribe(Rc::new(move |e: &Engine, c: &ModelChange| {
                    if let Some(s) = weak.upgrade() {
                        Shared::on_change(&s, e, c);
                    }
                }));
                let mut st = s.state.borrow_mut();
                st.subscription = id;
                st.model = Some(m.clone());
            }
            Shared::populate(s, engine);
        });
    }

    /// Whether delegates are made in the engine's incubation queue
    /// rather than on the spot. Applies to those made from now on.
    pub fn set_asynchronous(&self, on: bool) {
        self.shared.state.borrow_mut().asynchronous = on;
    }

    /// The number of rows: those whose delegate is still being made, or
    /// could not be, count too.
    pub fn count(&self) -> usize {
        self.shared.state.borrow().rows.len()
    }

    /// The finished delegate of row `index`.
    pub fn object_at(&self, index: usize) -> Option<ObjectId> {
        self.shared.state.borrow().rows.get(index)?.object
    }

    /// The finished delegates, in row order.
    pub fn objects(&self) -> Vec<ObjectId> {
        let st = self.shared.state.borrow();
        st.rows.iter().filter_map(|r| r.object).collect()
    }

    /// How many delegates were ever made: what a test counts to see that
    /// a change touched only its rows.
    pub fn created(&self) -> u64 {
        self.shared.state.borrow().created
    }
}

impl Shared {
    fn populate(s: &Rc<Shared>, engine: &Engine) {
        let ids = {
            let mut st = s.state.borrow_mut();
            let n = st.model.as_ref().map_or(0, |m| m.row_count());
            st.placeholders(0, n)
        };
        s.host.count_changed(engine, ids.len());
        for id in ids {
            Shared::create_row(s, engine, id);
        }
    }

    fn remove_all(&self, engine: &Engine) {
        let rows = {
            let mut st = self.state.borrow_mut();
            st.later.set(None);
            std::mem::take(&mut st.rows)
        };
        let rows = rows.into_iter().enumerate();
        match self.removal {
            RemovalOrder::FirstToLast => rows.for_each(|(i, r)| self.dispose(engine, i, r)),
            RemovalOrder::LastToFirst => rows.rev().for_each(|(i, r)| self.dispose(engine, i, r)),
        }
    }

    /// Takes the `count` rows from `first` out as their model did: each
    /// reads index -1 before the rest are renumbered, and each leaves
    /// from `first`, which is where it is by the time it goes.
    fn remove_rows(&self, engine: &Engine, first: usize, count: usize) {
        let (gone, n) = {
            let mut st = self.state.borrow_mut();
            let first = first.min(st.rows.len());
            let end = first.saturating_add(count).min(st.rows.len());
            st.later.set(None);
            let gone: Vec<Row> = st.rows.drain(first..end).collect();
            (gone, st.rows.len())
        };
        if gone.is_empty() {
            return;
        }
        for row in &gone {
            if let Some(d) = &row.data {
                // Before the index: what runs because the index changed
                // already finds the roles gone, as in Qt.
                engine.empty_row(d, row.object);
                if let Some(p) = d.shape.index {
                    let _ = engine.write_internal(d.id, p, Value::Int(-1), WriteKind::Imperative);
                }
            }
        }
        self.reindex(engine, first, n);
        self.host.count_changed(engine, n);
        for row in gone {
            self.dispose(engine, first, row);
        }
    }

    /// Ends a row that is already out of the list.
    fn dispose(&self, engine: &Engine, index: usize, row: Row) {
        if let Some(h) = &row.pending {
            h.cancel();
        }
        if let Some(obj) = row.object {
            self.host.removed(engine, index, obj);
            // `Component.onDestruction` runs now, before the next row
            // goes, as in Qt. The object itself goes later: the handler
            // that removed the row may be the delegate's own.
            engine.tell_destruction(obj, false);
            engine.destroy(obj);
        }
    }

    fn create_row(s: &Rc<Shared>, engine: &Engine, id: u64) {
        let (component, asynchronous, index, model) = {
            let st = s.state.borrow();
            (
                st.component,
                st.asynchronous,
                st.position(id),
                st.model.clone(),
            )
        };
        let (Some(component), Some(index), Some(model)) = (component, index, model) else {
            return;
        };
        let Some(component) = crate::chooser::resolve_delegate(engine, component, &model, index)
        else {
            return;
        };
        let for_setup = s.clone();
        let setup = move |e: &Engine, root: ObjectId, ctx: ContextId| {
            for_setup.setup_row(e, id, root, ctx);
        };
        if !asynchronous {
            let r = engine.create_delegate(component, Some(s.owner), setup);
            s.finish_row(engine, id, r);
            return;
        }
        let (job, handle) = CreationJob::for_delegate(
            engine,
            component,
            Some(s.owner),
            Lane::Visible,
            ProcId::new(0, 0),
            setup,
        );
        {
            let mut st = s.state.borrow_mut();
            if let Some(i) = st.position(id) {
                st.rows[i].pending = Some(handle);
            }
        }
        let weak = Rc::downgrade(s);
        engine.incubate(job.on_done(move |e, r| {
            // A job whose owner went ends with an error nobody is to
            // hear of: the view is gone, not the delegate wrong.
            if let Some(s) = weak.upgrade()
                && e.is_alive(s.owner)
            {
                s.finish_row(e, id, r.clone());
            }
        }));
    }

    /// Gives a delegate that was just allocated its row.
    fn setup_row(&self, engine: &Engine, id: u64, root: ObjectId, ctx: ContextId) {
        let found = {
            let st = self.state.borrow();
            st.position(id).and_then(|i| {
                let before = st.object_from(i + 1);
                Some((i, st.model.clone()?, before))
            })
        };
        let Some((index, model, before)) = found else {
            return;
        };
        if let Some(data) = give_row(engine, &model, index, root, ctx) {
            let mut st = self.state.borrow_mut();
            if let Some(i) = st.position(id) {
                st.rows[i].data = Some(data);
            }
        }
        self.host.allocated(engine, index, root, before);
    }

    fn finish_row(&self, engine: &Engine, id: u64, made: Result<ObjectId, EngineError>) {
        let index = {
            let mut st = self.state.borrow_mut();
            let i = st.position(id);
            if let Some(i) = i {
                st.rows[i].pending = None;
                match &made {
                    Ok(obj) => st.set_object(i, *obj),
                    // The row object went with the failed delegate.
                    Err(_) => st.rows[i].data = None,
                }
            }
            if made.is_ok() {
                st.created += 1;
            }
            i
        };
        match (made, index) {
            (Ok(obj), Some(i)) => self.host.added(engine, i, obj),
            // The row went while its delegate was being made.
            (Ok(obj), None) => {
                engine.destroy(obj);
            }
            (Err(_), None) => {}
            (Err(e), Some(_)) => {
                engine.log(
                    LogLevel::Info,
                    "qml",
                    "QML Component: Cannot create delegate",
                );
                engine.warn(match e {
                    // The message is whole already.
                    EngineError::RequiredNotSet(m) => m,
                    e => e.to_string(),
                });
            }
        }
    }

    fn on_change(s: &Rc<Shared>, engine: &Engine, change: &ModelChange) {
        if !engine.is_alive(s.owner) {
            return;
        }
        match change {
            // Nothing is kept by what is about to change.
            ModelChange::RowsAboutToBeInserted { .. }
            | ModelChange::RowsAboutToBeRemoved { .. } => {}
            ModelChange::RowsInserted { first, count } => {
                let (ids, n) = {
                    let mut st = s.state.borrow_mut();
                    let ids = st.placeholders(*first, *count);
                    (ids, st.rows.len())
                };
                s.reindex(engine, first + count, n);
                s.host.count_changed(engine, n);
                // One batch, so what watches all the delegates (a
                // positioner) runs once for the lot.
                engine.batch(|| {
                    for id in ids {
                        Shared::create_row(s, engine, id);
                    }
                });
            }
            ModelChange::RowsRemoved { first, count } => s.remove_rows(engine, *first, *count),
            ModelChange::RowsMoved { from, to, count } => {
                {
                    let mut st = s.state.borrow_mut();
                    let n = st.rows.len();
                    if from + count > n || to + count > n {
                        return;
                    }
                    st.later.set(None);
                    let moved: Vec<Row> = st.rows.drain(*from..from + count).collect();
                    st.rows.splice(*to..*to, moved);
                }
                s.reindex(engine, *from.min(to), from.max(to) + count);
                s.host.moved(engine);
            }
            ModelChange::DataChanged {
                first,
                count,
                roles,
            } => {
                let remake = s.state.borrow().component.is_some_and(|c| {
                    crate::chooser::is_chooser(engine, c)
                        && crate::chooser::role_affected(engine, c, roles)
                });
                if remake {
                    Shared::remake_rows(s, engine, *first, *count);
                } else {
                    s.refresh(engine, *first, *count, roles);
                }
            }
            ModelChange::RolesChanged => s.reshape(engine),
            ModelChange::ModelReset => engine.batch(|| {
                s.remove_all(engine);
                Shared::populate(s, engine);
            }),
        }
    }

    /// Destroys and recreates the delegates of `count` rows from `first`
    /// in place (for a [`DelegateChooser`] whose role changed).
    fn remake_rows(s: &Rc<Shared>, engine: &Engine, first: usize, count: usize) {
        engine.batch(|| {
            let end = {
                let st = s.state.borrow();
                first.saturating_add(count).min(st.rows.len())
            };
            for i in first.min(end)..end {
                let (old, id) = {
                    let mut st = s.state.borrow_mut();
                    if i >= st.rows.len() {
                        break;
                    }
                    let id = st.next_id;
                    st.next_id += 1;
                    st.later.set(None);
                    let old = std::mem::replace(
                        &mut st.rows[i],
                        Row {
                            id,
                            data: None,
                            object: None,
                            pending: None,
                        },
                    );
                    (old, id)
                };
                s.dispose(engine, i, old);
                Shared::create_row(s, engine, id);
            }
        });
    }

    /// Tells the rows in `from..to` where they are now.
    fn reindex(&self, engine: &Engine, from: usize, to: usize) {
        let writes: Vec<(ObjectId, PropIndex, usize)> = {
            let st = self.state.borrow();
            let to = to.min(st.rows.len());
            (from..to)
                .filter_map(|i| {
                    let d = st.rows[i].data.as_ref()?;
                    Some((d.id, d.shape.index?, i))
                })
                .collect()
        };
        for (obj, p, i) in writes {
            let _ = engine.write_internal(obj, p, Value::Int(i as i32), WriteKind::Imperative);
        }
    }

    /// Brings the row objects of `count` rows from `first` up to date.
    fn refresh(&self, engine: &Engine, first: usize, count: usize, roles: &[crate::Atom]) {
        let (model, rows): (_, Vec<(usize, RowObject)>) = {
            let st = self.state.borrow();
            let Some(model) = st.model.clone() else {
                return;
            };
            let end = (first + count).min(st.rows.len());
            let rows = (first.min(end)..end)
                .filter_map(|i| Some((i, st.rows[i].data.clone()?)))
                .filter(|(_, d)| !d.aliased)
                .collect();
            (model, rows)
        };
        for (i, d) in rows {
            refresh_row(engine, &model, i, &d, roles);
        }
    }

    /// The model has more roles: the row objects take the wider type.
    fn reshape(&self, engine: &Engine) {
        let (model, rows): (_, Vec<(u64, usize, RowObject)>) = {
            let st = self.state.borrow();
            let Some(model) = st.model.clone() else {
                return;
            };
            let rows = st
                .rows
                .iter()
                .enumerate()
                .filter_map(|(i, r)| Some((r.id, i, r.data.clone()?)))
                .filter(|(_, _, d)| !d.aliased)
                .collect();
            (model, rows)
        };
        let shape = row_shape(RowKind::Delegate, &model.role_names());
        for (id, i, old) in rows {
            if !reshape_row(engine, &model, &shape, i, &old) {
                continue;
            }
            let mut st = self.state.borrow_mut();
            if let Some(pos) = st.position(id)
                && let Some(d) = &mut st.rows[pos].data
            {
                d.shape = shape.clone();
            }
        }
    }
}

/// Gives the delegate `root`, just allocated in context `ctx`, row
/// `index` of `model`: a row object as its context object, or bound to
/// its required properties.
fn give_row(
    engine: &Engine,
    model: &Rc<dyn ItemModel>,
    index: usize,
    root: ObjectId,
    ctx: ContextId,
) -> Option<RowObject> {
    let shape = row_shape(RowKind::Delegate, &model.role_names());
    // Owned by the delegate: it lives exactly as long.
    let data = engine.make_row(model, &shape, index, Some(root)).ok()?;
    let required = required_properties(engine, root);
    if required.is_empty() {
        engine.set_context_object(ctx, Some(data.id));
    } else {
        let mut members = None;
        for (p, name) in required {
            let row = data.id;
            if let Some(from) = data.shape.meta.property_index(&name) {
                let _ = engine.bind(root, p, move |e| e.read(row, from));
                continue;
            }
            let Some(from) = data.shape.model_data else {
                continue;
            };
            let members = members.get_or_insert_with(|| object_members(engine, model, index));
            if members.contains(&name) {
                // A row without the member gives the property's own
                // default, as Qt does: `required property int d` is 0.
                let _ = engine.bind(root, p, move |e| {
                    let v = e.read(row, from);
                    e.js_member(&v, &name)
                        .unwrap_or_else(|| e.untrack(|| e.default_value(root, p)))
                });
            }
        }
    }
    Some(data)
}

/// The names a row of an array of JS objects gives required properties,
/// as Qt 6.11.2 does (found by running qml6): the members of the first
/// row and those of the row itself. A row without one of the first row's
/// members gives `undefined`.
fn object_members(engine: &Engine, model: &Rc<dyn ItemModel>, index: usize) -> Vec<String> {
    let keys = |row: usize| {
        model
            .model_data(engine, row)
            .map(|v| engine.untrack(|| engine.js_object_entries(&v)))
            .unwrap_or_default()
            .into_iter()
            .map(|(k, _)| k)
    };
    let mut names: Vec<String> = keys(0).collect();
    if index != 0 {
        for k in keys(index) {
            if !names.contains(&k) {
                names.push(k);
            }
        }
    }
    names
}

/// The required properties of a delegate's root.
fn required_properties(engine: &Engine, root: ObjectId) -> Vec<(PropIndex, String)> {
    let Some(meta) = engine.meta_of(root) else {
        return Vec::new();
    };
    meta.properties
        .iter()
        .enumerate()
        .filter(|(_, p)| p.flags.contains(PropertyFlags::REQUIRED))
        .map(|(i, p)| (PropIndex(i as u32), p.name.to_string()))
        .collect()
}

/// Brings the row object `d`, which shows row `i`, up to date in `roles`
/// (all of them when there are none) and in `modelData`.
fn refresh_row(
    engine: &Engine,
    model: &Rc<dyn ItemModel>,
    i: usize,
    d: &RowObject,
    roles: &[crate::Atom],
) {
    if d.aliased {
        return;
    }
    for (name, p) in &d.shape.roles {
        if roles.is_empty() || roles.contains(name) {
            let v = model.data(engine, i, name);
            let _ = engine.write_internal(d.id, *p, v, WriteKind::Imperative);
        }
    }
    if let Some(p) = d.shape.model_data {
        let v = model.model_data(engine, i).unwrap_or_default();
        let _ = engine.write_internal(d.id, p, v, WriteKind::Imperative);
    }
}

/// Gives the row object `old`, which shows row `i`, the wider type
/// `shape`. False when it could not take it.
fn reshape_row(
    engine: &Engine,
    model: &Rc<dyn ItemModel>,
    shape: &Rc<RowShape>,
    i: usize,
    old: &RowObject,
) -> bool {
    if engine.extend_meta(old.id, shape.meta.clone()).is_err() {
        return false;
    }
    for (name, p) in &shape.roles {
        if old.shape.role(name).is_none() {
            let v = model.data(engine, i, name);
            let _ = engine.write_internal(old.id, *p, v, WriteKind::Imperative);
        }
    }
    if let Some(p) = shape.model_data {
        let v = model.model_data(engine, i).unwrap_or_default();
        let _ = engine.write_internal(old.id, p, v, WriteKind::Imperative);
    }
    engine.set_write_hook(old.id, Some(row_hook(model.clone(), shape.clone(), old.id)));
    true
}

impl Engine {
    /// Makes one object of `component` for a view that is not a row: a
    /// header, a section heading, a highlight.
    ///
    /// `values` are what the object is given by name (`section` for a
    /// section heading). As for a delegate: a required property named like
    /// one takes it, and an object without required properties finds them
    /// in its context. `allocated` gets the root before anything in it
    /// ran: the place to give it its visual parent.
    pub fn create_in_context(
        &self,
        component: ObjectId,
        owner: Option<ObjectId>,
        values: &[(&str, Value)],
        allocated: impl FnOnce(&Engine, ObjectId),
    ) -> Result<ObjectId, EngineError> {
        self.create_delegate(component, owner, |e, root, ctx| {
            let required = required_properties(e, root);
            if required.is_empty() && !values.is_empty() {
                let roles: Vec<crate::Atom> =
                    values.iter().map(|(n, _)| crate::Atom::intern(n)).collect();
                let shape = row_shape(RowKind::Element, &roles);
                let row = e.create_with(
                    shape.meta.clone(),
                    Box::new(ModelRow::default()),
                    Some(root),
                );
                if let Ok(row) = row {
                    for (name, v) in values {
                        if let Some(p) = shape.role(name) {
                            let _ = e.write_internal(row, p, v.clone(), WriteKind::Initial);
                        }
                    }
                    e.set_context_object(ctx, Some(row));
                }
            }
            for (p, name) in required {
                if let Some((_, v)) = values.iter().find(|(n, _)| *n == name) {
                    let _ = e.write_internal(root, p, v.clone(), WriteKind::Initial);
                }
            }
            allocated(e, root);
        })
    }
}

// ---------------------------------------------------------------------
// Delegates for the rows a view asks for.

/// How soon a view wants the delegate of a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Urgency {
    /// Before the call returns: the row is to be shown.
    Now,
    /// In the engine's incubation queue, a slice a frame: the row is in
    /// the cache around what is shown. [`ViewHost::ready`] says when.
    Later,
}

/// Where the delegate [`ViewDelegates::request`] gave came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Made {
    /// The row had it already.
    Existing,
    /// It was made for the row.
    Created,
    /// It showed another row before and was in the pool.
    Reused,
}

/// What a virtualised view is told about its delegates. Called with
/// nothing borrowed: an implementation may call back into its
/// [`ViewDelegates`].
pub trait ViewHost {
    /// The root of the delegate for row `index` exists and nothing in it
    /// has run yet: the place to give it a visual parent, so that
    /// `parent` reads right inside it.
    fn allocated(&self, engine: &Engine, index: usize, object: ObjectId) {
        let _ = (engine, index, object);
    }

    /// A delegate asked for with [`Urgency::Later`] is complete.
    fn ready(&self, engine: &Engine, index: usize, object: ObjectId) {
        let _ = (engine, index, object);
    }

    /// The model changed, and the delegates' rows and indexes are
    /// already in step. `gone` are the delegates whose rows left, with
    /// the index each had: they read index -1 and stay alive until the
    /// view gives them back with [`ViewDelegates::release`].
    fn changed(&self, engine: &Engine, change: &ModelChange, gone: &[(usize, ObjectId)]);

    /// `object` is about to be destroyed. Also told of a row that shows
    /// itself ([`DelegateSource::Rows`]) when the view is done with it:
    /// that object stays alive.
    fn destroying(&self, engine: &Engine, object: ObjectId) {
        let _ = (engine, object);
    }
}

struct Held {
    /// Finds the entry again after user code ran.
    id: u64,
    /// `None`: the row left the model and the view still holds the
    /// delegate (a removal that is delayed or animated).
    index: Option<usize>,
    data: Option<RowObject>,
    /// The delegate, once complete.
    object: Option<ObjectId>,
    /// The delegate from the moment it is allocated: what runs inside it
    /// while it is being made may ask which row it is.
    root: Option<ObjectId>,
    pending: Option<(CreationHandle, SharedCreation)>,
    /// The component could not make it: asking again would only say the
    /// same again.
    failed: bool,
    /// The row's own object, not something made for it: never destroyed
    /// and never shown for another row.
    own: bool,
}

#[derive(Default)]
struct ViewState {
    model: Option<Rc<dyn ItemModel>>,
    subscription: u64,
    component: Option<ObjectId>,
    live: Vec<Held>,
    /// Delegates with no row, kept to show another (`reuseItems`).
    pool: Vec<Held>,
    next_id: u64,
    created: u64,
}

struct ViewShared {
    owner: ObjectId,
    host: Rc<dyn ViewHost>,
    state: RefCell<ViewState>,
    /// A delegate is being finished because the view asked for it now:
    /// the view has it from the call and needs no [`ViewHost::ready`].
    forcing: Cell<bool>,
}

impl Drop for ViewShared {
    fn drop(&mut self) {
        let st = self.state.get_mut();
        if let Some(m) = st.model.take() {
            m.unsubscribe(st.subscription);
        }
    }
}

/// The delegates of a view that has them only for the rows it shows
/// (ARCHITECTURE.md §8.4). The view asks for a row's delegate and gives
/// it back when the row is out of sight; this keeps every delegate's row
/// in step with the model in between. Cloning gives another handle to
/// the same set.
#[derive(Clone)]
pub struct ViewDelegates {
    shared: Rc<ViewShared>,
}

impl std::fmt::Debug for ViewDelegates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewDelegates")
            .field("owner", &self.shared.owner)
            .field("live", &self.shared.state.borrow().live.len())
            .finish()
    }
}

impl ViewDelegates {
    /// Delegates owned by `owner` (the view object).
    pub fn new(owner: ObjectId, host: Rc<dyn ViewHost>) -> Self {
        ViewDelegates {
            shared: Rc::new(ViewShared {
                owner,
                host,
                state: RefCell::new(ViewState::default()),
                forcing: Cell::new(false),
            }),
        }
    }

    /// A new model or delegate component: every delegate there is goes.
    /// Without either there are no rows.
    pub fn set_source(
        &self,
        engine: &Engine,
        model: Option<Rc<dyn ItemModel>>,
        component: Option<ObjectId>,
    ) {
        let s = &self.shared;
        let model =
            model.filter(|m| component.is_some() || m.delegate_source() != DelegateSource::View);
        s.destroy_all(engine);
        let old = {
            let mut st = s.state.borrow_mut();
            st.component = component;
            st.model.take().map(|m| (m, st.subscription))
        };
        if let Some((m, id)) = old {
            m.unsubscribe(id);
        }
        if let Some(m) = &model {
            let weak = Rc::downgrade(s);
            let id = m.subscribe(Rc::new(move |e: &Engine, c: &ModelChange| {
                if let Some(s) = weak.upgrade() {
                    ViewShared::on_change(&s, e, c);
                }
            }));
            let mut st = s.state.borrow_mut();
            st.subscription = id;
            st.model = Some(m.clone());
        }
    }

    /// The number of rows. None while nothing can make their delegates.
    pub fn count(&self) -> usize {
        let st = self.shared.state.borrow();
        match &st.model {
            Some(m) if m.delegate_source() == DelegateSource::Component(None) => {
                usize::from(st.component.is_some()) * m.row_count()
            }
            Some(m) => m.row_count(),
            None => 0,
        }
    }

    /// Whether the rows are objects that show themselves: there is
    /// nothing to make, so nothing is saved by leaving rows out.
    pub fn rows_show_themselves(&self) -> bool {
        let st = self.shared.state.borrow();
        st.model
            .as_ref()
            .is_some_and(|m| m.delegate_source() == DelegateSource::Rows)
    }

    pub fn model(&self) -> Option<Rc<dyn ItemModel>> {
        self.shared.state.borrow().model.clone()
    }

    /// What identifies row `index` wherever it moves.
    pub fn key_at(&self, index: usize) -> Option<RowKey> {
        let model = self.model()?;
        (index < model.row_count()).then(|| model.row_key(index))
    }

    /// Role `role` of row `index`, straight from the model. For a row
    /// that is an object, that property of it.
    pub fn role(&self, engine: &Engine, index: usize, role: &str) -> Value {
        let Some(model) = self.model() else {
            return Value::Undefined;
        };
        match model.data(engine, index, role) {
            Value::Undefined => match model.model_data(engine, index) {
                Some(Value::Object(o)) => engine.read_by_name(o, role),
                _ => Value::Undefined,
            },
            v => v,
        }
    }

    /// The complete delegate of row `index`, when the row has one.
    pub fn object_at(&self, index: usize) -> Option<ObjectId> {
        let st = self.shared.state.borrow();
        st.live
            .iter()
            .find(|h| h.index == Some(index))
            .and_then(|h| h.object)
    }

    /// The row `object` is the delegate of. Also answers while the
    /// delegate is being made.
    pub fn index_of(&self, object: ObjectId) -> Option<usize> {
        let st = self.shared.state.borrow();
        st.live
            .iter()
            .find(|h| h.object == Some(object) || h.root == Some(object))
            .and_then(|h| h.index)
    }

    /// Whether the delegate of row `index` is being made.
    pub fn is_pending(&self, index: usize) -> bool {
        let st = self.shared.state.borrow();
        st.live
            .iter()
            .any(|h| h.index == Some(index) && h.pending.is_some())
    }

    /// The rows that have a complete delegate, in no order.
    pub fn live(&self) -> Vec<(usize, ObjectId)> {
        let st = self.shared.state.borrow();
        st.live
            .iter()
            .filter_map(|h| Some((h.index?, h.object?)))
            .collect()
    }

    /// The rows whose delegate is being made.
    pub fn pending(&self) -> Vec<usize> {
        let st = self.shared.state.borrow();
        st.live
            .iter()
            .filter(|h| h.pending.is_some())
            .filter_map(|h| h.index)
            .collect()
    }

    /// How many delegates were ever made: what a test counts to see that
    /// a view makes them only for what it shows.
    pub fn created(&self) -> u64 {
        self.shared.state.borrow().created
    }

    /// How many delegates wait in the pool.
    pub fn pooled(&self) -> usize {
        self.shared.state.borrow().pool.len()
    }

    /// The delegate of row `index`: the one the row has, one from the
    /// pool, or a new one. `None` when it is still being made
    /// ([`Urgency::Later`]), cannot be made, or there is no such row.
    pub fn request(
        &self,
        engine: &Engine,
        index: usize,
        urgency: Urgency,
    ) -> Option<(ObjectId, Made)> {
        let s = &self.shared;
        enum Have {
            Object(ObjectId),
            Pending(u64, SharedCreation),
            Failed,
            Nothing,
        }
        let (have, model, component) = {
            let st = s.state.borrow();
            let model = st.model.clone()?;
            if index >= model.row_count() {
                return None;
            }
            let have = match st.live.iter().find(|h| h.index == Some(index)) {
                Some(h) => match (&h.object, &h.pending) {
                    (Some(o), _) => Have::Object(*o),
                    (None, Some((_, job))) => Have::Pending(h.id, job.clone()),
                    (None, None) => Have::Failed,
                },
                None => Have::Nothing,
            };
            (have, model, st.component)
        };
        match have {
            Have::Object(o) => return Some((o, Made::Existing)),
            Have::Failed => return None,
            Have::Pending(id, job) => {
                if urgency == Urgency::Later {
                    return None;
                }
                let was = s.forcing.replace(true);
                job.force();
                s.forcing.set(was);
                let st = s.state.borrow();
                let made = st.live.iter().find(|h| h.id == id).and_then(|h| h.object);
                return made.map(|o| (o, Made::Created));
            }
            Have::Nothing => {}
        }
        let component = match model.delegate_source() {
            DelegateSource::View => component?,
            // As Qt: what the view itself was given comes first.
            DelegateSource::Component(c) => component.or(c)?,
            DelegateSource::Rows => return s.adopt(engine, &model, index),
        };
        let is_chooser = crate::chooser::is_chooser(engine, component);
        let component = crate::chooser::resolve_delegate(engine, component, &model, index)?;
        // A row that is an object shows that object's own properties: a
        // delegate made for another row cannot be pointed at it.
        if !is_chooser && model.row_object(index).is_none() {
            let reused = {
                let mut st = s.state.borrow_mut();
                st.pool.pop().map(|mut h| {
                    h.index = Some(index);
                    let out = (h.object, h.data.clone());
                    st.live.push(h);
                    out
                })
            };
            if let Some((Some(object), Some(data))) = reused {
                engine.batch(|| {
                    if let Some(p) = data.shape.index {
                        let v = Value::Int(index as i32);
                        let _ = engine.write_internal(data.id, p, v, WriteKind::Imperative);
                    }
                    refresh_row(engine, &model, index, &data, &[]);
                });
                return Some((object, Made::Reused));
            }
        }
        let id = {
            let mut st = s.state.borrow_mut();
            st.next_id += 1;
            let id = st.next_id;
            st.live.push(Held {
                id,
                index: Some(index),
                data: None,
                object: None,
                root: None,
                pending: None,
                failed: false,
                own: false,
            });
            id
        };
        let for_setup = s.clone();
        let setup = move |e: &Engine, root: ObjectId, ctx: ContextId| {
            for_setup.setup(e, id, root, ctx);
        };
        match urgency {
            Urgency::Now => {
                let r = engine.create_delegate(component, Some(s.owner), setup);
                s.finish(engine, id, r, false).map(|o| (o, Made::Created))
            }
            Urgency::Later => {
                let (job, handle) = CreationJob::for_delegate(
                    engine,
                    component,
                    Some(s.owner),
                    Lane::Prefetch,
                    ProcId::new(0, 0),
                    setup,
                );
                let weak = Rc::downgrade(s);
                let (job, shared) = job
                    .on_done(move |e, r| {
                        // A job whose owner went ends with an error
                        // nobody is to hear of.
                        if let Some(s) = weak.upgrade()
                            && e.is_alive(s.owner)
                        {
                            let tell = !s.forcing.get();
                            s.finish(e, id, r.clone(), tell);
                        }
                    })
                    .shared();
                {
                    let mut st = s.state.borrow_mut();
                    if let Some(h) = st.live.iter_mut().find(|h| h.id == id) {
                        h.pending = Some((handle, shared));
                    }
                }
                engine.incubate(job);
                None
            }
        }
    }

    /// The view is done with `object`. With `reuse` it goes to the pool
    /// to show another row later, and the result is true; otherwise, or
    /// when it cannot be reused, it is destroyed.
    pub fn release(&self, engine: &Engine, object: ObjectId, reuse: bool) -> bool {
        let s = &self.shared;
        let held = {
            let mut st = s.state.borrow_mut();
            let Some(i) = st.live.iter().position(|h| h.object == Some(object)) else {
                return false;
            };
            let chooser = st
                .component
                .is_some_and(|c| crate::chooser::is_chooser(engine, c));
            let mut h = st.live.swap_remove(i);
            let poolable =
                reuse && !chooser && !h.own && h.data.as_ref().is_some_and(|d| !d.aliased);
            if poolable {
                h.index = None;
                st.pool.push(h);
                return true;
            }
            h
        };
        s.destroy(engine, held);
        false
    }

    /// Calls off the delegate being made for row `index`.
    pub fn cancel(&self, engine: &Engine, index: usize) {
        let s = &self.shared;
        let held = {
            let mut st = s.state.borrow_mut();
            let found = st
                .live
                .iter()
                .position(|h| h.index == Some(index) && h.object.is_none());
            found.map(|i| st.live.swap_remove(i))
        };
        if let Some(h) = held {
            s.destroy(engine, h);
        }
    }

    /// Destroys the pooled delegates beyond the first `keep`.
    pub fn trim_pool(&self, engine: &Engine, keep: usize) {
        let s = &self.shared;
        let extra: Vec<Held> = {
            let mut st = s.state.borrow_mut();
            let keep = keep.min(st.pool.len());
            st.pool.drain(keep..).collect()
        };
        for h in extra {
            s.destroy(engine, h);
        }
    }
}

impl ViewShared {
    /// The object of row `index` of a model whose rows show themselves.
    fn adopt(
        &self,
        engine: &Engine,
        model: &Rc<dyn ItemModel>,
        index: usize,
    ) -> Option<(ObjectId, Made)> {
        let object = model.row_object(index).filter(|o| engine.is_alive(*o))?;
        {
            let mut st = self.state.borrow_mut();
            // Taken out and put back in while the view still held it.
            if let Some(h) = st.live.iter_mut().find(|h| h.object == Some(object)) {
                h.index = Some(index);
                return Some((object, Made::Existing));
            }
            st.next_id += 1;
            let id = st.next_id;
            st.live.push(Held {
                id,
                index: Some(index),
                data: None,
                object: Some(object),
                root: Some(object),
                pending: None,
                failed: false,
                own: true,
            });
        }
        self.host.allocated(engine, index, object);
        Some((object, Made::Created))
    }

    fn destroy(&self, engine: &Engine, held: Held) {
        if held.own {
            if let Some(obj) = held.object {
                self.host.destroying(engine, obj);
            }
            return;
        }
        if let Some((handle, _)) = &held.pending {
            // What the job made so far goes at its next step; the view
            // is told now, while the object is still there to unparent.
            if let Some(half) = handle.half_made() {
                self.host.destroying(engine, half);
            }
            handle.cancel();
        }
        if let Some(obj) = held.object {
            self.host.destroying(engine, obj);
            let model = self.state.borrow().model.clone();
            if let Some(m) = model {
                m.delegate_gone(engine, obj);
            }
            engine.tell_destruction(obj, false);
            engine.destroy(obj);
        }
    }

    fn destroy_all(&self, engine: &Engine) {
        let all: Vec<Held> = {
            let mut st = self.state.borrow_mut();
            let mut all = std::mem::take(&mut st.live);
            all.append(&mut st.pool);
            all
        };
        for h in all {
            self.destroy(engine, h);
        }
    }

    /// Gives a delegate that was just allocated its row.
    fn setup(&self, engine: &Engine, id: u64, root: ObjectId, ctx: ContextId) {
        let found = {
            let st = self.state.borrow();
            let held = st.live.iter().find(|h| h.id == id);
            held.and_then(|h| Some((h.index?, st.model.clone()?)))
        };
        let Some((index, model)) = found else {
            return;
        };
        let data = give_row(engine, &model, index, root, ctx);
        {
            let mut st = self.state.borrow_mut();
            if let Some(h) = st.live.iter_mut().find(|h| h.id == id) {
                h.root = Some(root);
                if data.is_some() {
                    h.data = data;
                }
            }
        }
        self.host.allocated(engine, index, root);
        model.delegate_allocated(engine, index, root);
    }

    fn finish(
        &self,
        engine: &Engine,
        id: u64,
        made: Result<ObjectId, EngineError>,
        tell: bool,
    ) -> Option<ObjectId> {
        let (index, model) = {
            let mut st = self.state.borrow_mut();
            let held = st.live.iter_mut().find(|h| h.id == id);
            let index = held.map(|h| {
                h.pending = None;
                match &made {
                    Ok(obj) => h.object = Some(*obj),
                    Err(_) => {
                        // The row object went with the failed delegate.
                        h.data = None;
                        h.failed = true;
                    }
                }
                h.index
            });
            if made.is_ok() && index.is_some() {
                st.created += 1;
            }
            (index, st.model.clone())
        };
        match (made, index) {
            (Ok(obj), Some(index)) => {
                if let (Some(index), Some(m)) = (index, &model) {
                    m.delegate_made(engine, index, obj);
                }
                if tell && let Some(index) = index {
                    self.host.ready(engine, index, obj);
                }
                Some(obj)
            }
            // The row went while its delegate was being made.
            (Ok(obj), None) => {
                self.host.destroying(engine, obj);
                engine.destroy(obj);
                None
            }
            (Err(_), None) => None,
            (Err(e), Some(_)) => {
                engine.log(
                    LogLevel::Info,
                    "qml",
                    "QML Component: Cannot create delegate",
                );
                engine.warn(match e {
                    // The message is whole already.
                    EngineError::RequiredNotSet(m) => m,
                    e => e.to_string(),
                });
                None
            }
        }
    }

    /// Moves every delegate to the row `to` says its row now is.
    fn remap(&self, engine: &Engine, to: impl Fn(usize) -> usize) {
        let writes: Vec<(ObjectId, PropIndex, usize)> = {
            let mut st = self.state.borrow_mut();
            let mut writes = Vec::new();
            for h in &mut st.live {
                let Some(old) = h.index else {
                    continue;
                };
                let new = to(old);
                if new == old {
                    continue;
                }
                h.index = Some(new);
                if let Some(d) = &h.data
                    && let Some(p) = d.shape.index
                {
                    writes.push((d.id, p, new));
                }
            }
            writes
        };
        for (obj, p, i) in writes {
            let _ = engine.write_internal(obj, p, Value::Int(i as i32), WriteKind::Imperative);
        }
    }

    fn on_change(s: &Rc<ViewShared>, engine: &Engine, change: &ModelChange) {
        if !engine.is_alive(s.owner) {
            return;
        }
        let mut gone: Vec<(usize, ObjectId)> = Vec::new();
        let mut chooser_remade = false;
        engine.batch(|| match change {
            ModelChange::RowsAboutToBeInserted { .. }
            | ModelChange::RowsAboutToBeRemoved { .. } => {}
            ModelChange::RowsInserted { first, count } => {
                s.remap(engine, |i| if i >= *first { i + count } else { i });
            }
            ModelChange::RowsRemoved { first, count } => {
                let end = first.saturating_add(*count);
                let inside = |h: &Held| h.index.is_some_and(|i| i >= *first && i < end);
                let (unmade, leaving) = {
                    let mut st = s.state.borrow_mut();
                    let mut unmade: Vec<Held> = Vec::new();
                    let mut i = 0;
                    while i < st.live.len() {
                        if inside(&st.live[i]) && st.live[i].object.is_none() {
                            unmade.push(st.live.swap_remove(i));
                        } else {
                            i += 1;
                        }
                    }
                    let mut leaving = Vec::new();
                    for h in st.live.iter_mut().filter(|h| inside(h)) {
                        if let (Some(i), Some(o)) = (h.index.take(), h.object) {
                            leaving.push((i, o, h.data.clone()));
                        }
                    }
                    leaving.sort_by_key(|(i, ..)| *i);
                    (unmade, leaving)
                };
                for h in unmade {
                    s.destroy(engine, h);
                }
                for (_, object, data) in &leaving {
                    if let Some(d) = data {
                        // Before the index: what runs because the index
                        // changed already finds the roles gone, as in Qt.
                        engine.empty_row(d, Some(*object));
                        if let Some(p) = d.shape.index {
                            let _ = engine.write_internal(
                                d.id,
                                p,
                                Value::Int(-1),
                                WriteKind::Imperative,
                            );
                        }
                    }
                }
                s.remap(engine, |i| if i >= end { i - count } else { i });
                gone.extend(leaving.iter().map(|(i, o, _)| (*i, *o)));
            }
            ModelChange::RowsMoved { from, to, count } => {
                let (from, to, count) = (*from, *to, *count);
                s.remap(engine, |i| {
                    if i >= from && i < from + count {
                        return to + (i - from);
                    }
                    let without = if i >= from + count { i - count } else { i };
                    if without >= to {
                        without + count
                    } else {
                        without
                    }
                });
            }
            ModelChange::DataChanged {
                first,
                count,
                roles,
            } => {
                let end = first.saturating_add(*count);
                let remake = {
                    let st = s.state.borrow();
                    let comp = st.model.as_ref().and_then(|m| match m.delegate_source() {
                        DelegateSource::View => st.component,
                        DelegateSource::Component(c) => st.component.or(c),
                        DelegateSource::Rows => None,
                    });
                    comp.is_some_and(|c| {
                        crate::chooser::is_chooser(engine, c)
                            && crate::chooser::role_affected(engine, c, roles)
                    })
                };
                if remake {
                    let removed: Vec<Held> = {
                        let mut st = s.state.borrow_mut();
                        let mut out = Vec::new();
                        let mut i = 0;
                        while i < st.live.len() {
                            if st.live[i]
                                .index
                                .is_some_and(|idx| idx >= *first && idx < end)
                            {
                                out.push(st.live.swap_remove(i));
                            } else {
                                i += 1;
                            }
                        }
                        out
                    };
                    for h in removed {
                        s.destroy(engine, h);
                    }
                    chooser_remade = true;
                } else {
                    let (model, rows): (_, Vec<(usize, RowObject)>) = {
                        let st = s.state.borrow();
                        let Some(model) = st.model.clone() else {
                            return;
                        };
                        let rows = st
                            .live
                            .iter()
                            .filter_map(|h| Some((h.index?, h.data.clone()?)))
                            .filter(|(i, _)| i >= first && *i < end)
                            .collect();
                        (model, rows)
                    };
                    for (i, d) in rows {
                        refresh_row(engine, &model, i, &d, roles);
                    }
                }
            }
            ModelChange::RolesChanged => {
                let (model, rows, pool): (_, Vec<(u64, usize, RowObject)>, Vec<Held>) = {
                    let mut st = s.state.borrow_mut();
                    let Some(model) = st.model.clone() else {
                        return;
                    };
                    let rows = st
                        .live
                        .iter()
                        .filter_map(|h| Some((h.id, h.index?, h.data.clone()?)))
                        .filter(|(_, _, d)| !d.aliased)
                        .collect();
                    // A pooled delegate has the narrower row: simpler to
                    // make a new one than to widen what shows no row.
                    (model, rows, std::mem::take(&mut st.pool))
                };
                for h in pool {
                    s.destroy(engine, h);
                }
                let shape = row_shape(RowKind::Delegate, &model.role_names());
                for (id, i, old) in rows {
                    if !reshape_row(engine, &model, &shape, i, &old) {
                        continue;
                    }
                    let mut st = s.state.borrow_mut();
                    if let Some(h) = st.live.iter_mut().find(|h| h.id == id)
                        && let Some(d) = &mut h.data
                    {
                        d.shape = shape.clone();
                    }
                }
            }
            ModelChange::ModelReset => s.destroy_all(engine),
        });
        s.host.changed(engine, change, &gone);
        if chooser_remade {
            s.host.ready(engine, 0, s.owner);
        }
    }
}
