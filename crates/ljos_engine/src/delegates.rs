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
use crate::incubator::{CreationHandle, CreationJob};
use crate::log::LogLevel;
use crate::meta::PropertyFlags;
use crate::model::{ItemModel, ModelChange, RowKind, RowObject, row_hook, row_shape};
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
        let (component, asynchronous) = {
            let st = s.state.borrow();
            (st.component, st.asynchronous)
        };
        let Some(component) = component else {
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
        let shape = row_shape(RowKind::Delegate, &model.role_names());
        // Owned by the delegate: it lives exactly as long.
        if let Ok(data) = engine.make_row(&model, &shape, index, Some(root)) {
            let required = required_properties(engine, root);
            if required.is_empty() {
                engine.set_context_object(ctx, Some(data.id));
            } else {
                for (p, name) in required {
                    let Some(from) = data.shape.meta.property_index(&name) else {
                        continue;
                    };
                    let row = data.id;
                    let _ = engine.bind(root, p, move |e| e.read(row, from));
                }
            }
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
            } => s.refresh(engine, *first, *count, roles),
            ModelChange::RolesChanged => s.reshape(engine),
            ModelChange::ModelReset => engine.batch(|| {
                s.remove_all(engine);
                Shared::populate(s, engine);
            }),
        }
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
            if engine.extend_meta(old.id, shape.meta.clone()).is_err() {
                continue;
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
            let mut st = self.state.borrow_mut();
            if let Some(pos) = st.position(id)
                && let Some(d) = &mut st.rows[pos].data
            {
                d.shape = shape.clone();
            }
        }
    }
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
