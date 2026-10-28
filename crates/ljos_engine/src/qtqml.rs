//! The QtQml module's non-visual types (contract items 3 and 4):
//! `Component`, `Connections` and `Timer`, plus the engine's timer service
//! (`Timer`, `Qt.callLater`, `destroy(delay)`).
//!
//! Timers live on a [`FrameScheduler`] read against a [`Clock`]. Nothing
//! polls: the host asks [`Engine::next_timer_deadline`] when to wake and
//! calls [`Engine::advance_timers`] then; tests use a `VirtualClock` and
//! call it directly.

use std::rc::Rc;
use std::time::Duration;

use ljos_js::{JsRef, JsValue, Scope};
use ljos_reactive::NodeState;
use ljos_runtime::{Clock, FrameScheduler, SystemClock, Time, TimerId, TimerKind};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::atom::Atom;
use crate::builtins::QtObject;
use crate::cx::Cx;
use crate::engine::Engine;
use crate::error::{EngineError, JsError};
use crate::ids::{ConnectionId, ContextId, ObjectId};
use crate::incubator::Initial;
use crate::incubator::Template;
use crate::meta::MetaRef;
use crate::object::{ObjectFlags, QmlType};
use crate::signal::Handler;
use crate::types::Url;
use crate::value::Value;
use crate::{QmlObject, WriteKind, qml_impl};

/// `Component.status` values, and how `Qt.createComponent` is asked to
/// load.
fn component_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "Status",
            vec![("Null", 0), ("Ready", 1), ("Loading", 2), ("Error", 3)],
        ),
        (
            "CompilationMode",
            vec![("PreferSynchronous", 0), ("Asynchronous", 1)],
        ),
    ]
}

/// `Locale` (QtQml): the names of what a locale's members take and give,
/// `Locale.ShortFormat`, `Locale.Monday`
/// (doc.qt.io/qt-6/qml-qtqml-locale.html). A locale itself is the value
/// `Qt.locale()` returns, never an object of this type. The numbers are
/// what Qt 6.11.2 answers for the keys; `Language`, `Script` and
/// `Territory`, hundreds of names each, are not here.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQml", name = "Locale", uncreatable, enums = locale_enums)]
pub struct LocaleEnums {}

fn locale_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "FormatType",
            vec![("LongFormat", 0), ("ShortFormat", 1), ("NarrowFormat", 2)],
        ),
        (
            "DayOfWeek",
            vec![
                ("Sunday", 0),
                ("Monday", 1),
                ("Tuesday", 2),
                ("Wednesday", 3),
                ("Thursday", 4),
                ("Friday", 5),
                ("Saturday", 6),
            ],
        ),
        (
            "MeasurementSystem",
            vec![
                ("MetricSystem", 0),
                ("ImperialUSSystem", 1),
                ("ImperialUKSystem", 2),
                ("ImperialSystem", 1),
            ],
        ),
        (
            "CurrencySymbolFormat",
            vec![
                ("CurrencyIsoCode", 0),
                ("CurrencySymbol", 1),
                ("CurrencyDisplayName", 2),
            ],
        ),
        (
            "DataSizeFormat",
            vec![
                ("DataSizeIecFormat", 0),
                ("DataSizeBase1000", 1),
                ("DataSizeSIQuantifiers", 2),
                ("DataSizeTraditionalFormat", 2),
                ("DataSizeSIFormat", 3),
            ],
        ),
        (
            "NumberOption",
            vec![
                ("DefaultNumberOptions", 0),
                ("OmitGroupSeparator", 1),
                ("RejectGroupSeparator", 2),
                ("OmitLeadingZeroInExponent", 4),
                ("RejectLeadingZeroInExponent", 8),
                ("IncludeTrailingZeroesAfterDot", 16),
                ("RejectTrailingZeroesAfterDot", 32),
            ],
        ),
    ]
}

/// `Component`: a template objects are made from (`Component { … }`, an
/// inline `component`, a property of type `Component`).
#[derive(QmlObject)]
#[qml(
    module = "QtQml",
    name = "Component",
    extends = QtObject,
    attached = ComponentAttached,
    enums = component_enums
)]
pub struct Component {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, readonly, enumeration = "Status")]
    pub status: i32,
    #[qml(readonly)]
    pub url: Url,
    /// Every component is made from a file read whole, or declared in
    /// place: loaded all the way, whatever its status, as qml6 says of a
    /// local file.
    #[qml(notify, readonly)]
    pub progress: f64,
    #[qml(skip)]
    pub(crate) template: Option<Rc<Template>>,
    /// Why there is no template: what `errorString()` says.
    #[qml(skip)]
    pub(crate) error: Atom,
}

impl Default for Component {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            status: 0,
            url: Url::default(),
            progress: 1.0,
            template: None,
            error: Atom::default(),
        }
    }
}

impl std::fmt::Debug for Component {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Component")
            .field("status", &self.status)
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

#[qml_impl]
impl Component {
    /// `createObject(parent, properties)`: `null` and a warning on failure,
    /// as in Qt.
    #[qml(invokable, name = "createObject")]
    fn create_object(cx: &Cx, parent: Value, props: Value) -> Value {
        let parent = parent.as_object();
        let Ok(template) = cx.engine.component_template(cx.this) else {
            cx.engine
                .warn("QQmlComponent: Component is not ready".to_owned());
            return Value::Null;
        };
        let props = cx.engine.js_object_entries(&props);
        match cx
            .engine
            .create_for_script(cx.this, template, parent, props, Initial::Created)
        {
            Ok(o) => {
                cx.engine.component_made(cx.this, None);
                Value::Object(o)
            }
            // Qt's words, which say where, a line for each property; the
            // message is whole already.
            Err(EngineError::RequiredNotSet(e)) => {
                for line in e.lines() {
                    cx.engine.warn(line.to_owned());
                }
                cx.engine.component_made(cx.this, Some(&e));
                Value::Null
            }
            Err(e) => {
                cx.engine.warn(format!("Component.createObject: {e}"));
                Value::Null
            }
        }
    }

    /// `incubateObject(parent, properties, mode)`: the incubator of an
    /// object made in slices (incubate.rs).
    #[qml(invokable, name = "incubateObject")]
    fn incubate_object(cx: &Cx, parent: Value, props: Value, mode: Value) -> Value {
        crate::incubate::incubate_object(cx, parent, props, mode)
    }

    #[qml(invokable, name = "errorString")]
    fn error_string(&self) -> Atom {
        self.error.clone()
    }
}

/// What `Component.` attaches: `onCompleted`, `onDestruction`, and
/// Ljós's `onReloaded` (see `incubator/reload.rs`).
///
/// Not `uncreatable`: the engine makes an attached object with the
/// attached type's constructor. It is in no module, so QML cannot name it.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "ComponentAttached")]
pub struct ComponentAttached {}

#[qml_impl]
impl ComponentAttached {
    #[qml(signal)]
    fn completed();

    #[qml(signal)]
    fn destruction();

    /// Never emitted: the engine runs `Component.onReloaded` itself, in
    /// place of `onCompleted` for an object a reload kept. Declared so
    /// that what reads QML knows the name.
    #[qml(signal)]
    fn reloaded();
}

/// One context's share of a destruction: the attached objects to tell,
/// and the contexts made inside it.
#[derive(Default)]
struct Farewell {
    parent: Option<ContextId>,
    serial: u64,
    attached: Vec<ObjectId>,
    inner: Vec<(u64, ContextId)>,
    done: bool,
}

impl Engine {
    /// Runs `Component.onDestruction` for `obj` and everything it owns,
    /// while all of it is still alive. [`Engine::destroy_now`] does this
    /// itself; call it earlier to let an object hear of its end before it
    /// is taken out of the tree (a `Loader` does). It runs once per
    /// object.
    ///
    /// The order is Qt's, found by running qml6: a component instance
    /// tells the objects written in its own file first, children before
    /// their parent, and then the instances of other components made
    /// inside it, the youngest first. An object that roots a component
    /// instance is told with that component, not with the file that uses
    /// it. Objects whose own file lives on (an inline child destroyed by
    /// hand) are not told at all.
    pub fn announce_destruction(&self, obj: ObjectId) {
        self.tell_destruction(obj, true);
    }

    /// [`Engine::announce_destruction`]. `outermost` starts with the
    /// outermost component `obj` roots, as a `Loader` does. A plain
    /// deletion in Qt starts one layer above the innermost and works
    /// outwards, which shows only for a root whose type derives from a
    /// component that derives from another.
    ///
    /// Once told, the object and what it owns are done: their bindings
    /// keep the values they had and their effects ([`Engine::effect_in`])
    /// stop, so nothing runs against a half torn-down object (a delegate
    /// that is already out of its view). Qt evaluates bindings as their
    /// inputs change, so what is still due is settled first.
    pub(crate) fn tell_destruction(&self, obj: ObjectId, outermost: bool) {
        self.tell_destruction_of(obj, outermost, false, true);
    }

    /// [`Engine::tell_destruction`] for an object a reload took out of
    /// its file: the file lives on, but the object is told as if its
    /// written place went with it.
    pub(crate) fn tell_removed(&self, obj: ObjectId) {
        self.tell_destruction_of(obj, false, true, true);
    }

    /// [`Engine::tell_destruction`] for an object destroyed now, which is
    /// gone before anything else runs: what it still owes is worked out
    /// only for a handler that is told it goes, as nothing else can read
    /// it. A board of blocks made and dropped before a frame was every
    /// binding of every block.
    pub(crate) fn tell_destruction_going(&self, obj: ObjectId) {
        self.tell_destruction_of(obj, false, false, false);
    }

    fn tell_destruction_of(&self, obj: ObjectId, outermost: bool, removed: bool, settle: bool) {
        let (ids, due) = {
            let store = self.inner.store.borrow();
            if store
                .objects
                .get(obj)
                .is_none_or(|h| h.flags.contains(ObjectFlags::DESTRUCTION_TOLD))
            {
                return;
            }
            let g = &self.inner.graph;
            // A delegate is a handful of objects, which these hold without
            // an allocation.
            let mut ids: SmallVec<[ObjectId; 8]> = SmallVec::new();
            let mut due = Vec::new();
            let mut stack: SmallVec<[ObjectId; 8]> = SmallVec::new();
            stack.push(obj);
            while let Some(id) = stack.pop() {
                let Some(h) = store.objects.get(id) else {
                    continue;
                };
                if id != obj && h.flags.contains(ObjectFlags::DESTRUCTION_TOLD) {
                    continue;
                }
                ids.push(id);
                stack.extend_from_slice(&h.children);
            }
            let settle = settle
                || self.inner.destruction_handlers.get()
                || self.inner.signals.borrow().any_destroyed_handler(&ids);
            if settle {
                for id in &ids {
                    // An object a reload replaces by one that goes on is
                    // not heard from as it goes, so what it still owes is
                    // not worked out.
                    if self.reload_muted(*id) {
                        continue;
                    }
                    let Some(h) = store.objects.get(*id) else {
                        continue;
                    };
                    due.extend(
                        h.nodes
                            .iter()
                            .flatten()
                            .flatten()
                            .filter(|n| g.state(**n).is_some_and(|s| s != NodeState::Clean)),
                    );
                }
            }
            (ids, due)
        };
        for n in due {
            let _ = self.untrack(|| self.inner.graph.try_get(n));
        }
        if self.inner.destruction_handlers.get() {
            self.forget_held_lists();
            self.run_destruction_handlers(obj, outermost, removed);
        }
        let mut store = self.inner.store.borrow_mut();
        for id in ids {
            if let Some(h) = store.objects.get_mut(id) {
                h.flags |= ObjectFlags::DESTRUCTION_TOLD;
            }
        }
    }

    fn run_destruction_handlers(&self, obj: ObjectId, outermost: bool, removed: bool) {
        let key = MetaRef::Static(<Component as QmlType>::static_meta()).type_id();
        // (the context it is told with, attached object), children before
        // their owner.
        let mut told: Vec<(ContextId, ObjectId)> = Vec::new();
        let mut rooted: Vec<ContextId> = Vec::new();
        let own: Vec<ContextId> = {
            let mut store = self.inner.store.borrow_mut();
            let Some(h) = store.objects.get_mut(obj) else {
                return;
            };
            if h.flags.contains(ObjectFlags::DESTRUCTION_TOLD) {
                return;
            }
            h.flags |= ObjectFlags::DESTRUCTION_TOLD;
            let own = h.ext.as_ref().map_or_else(Vec::new, |e| e.contexts.clone());
            if removed {
                rooted.push(h.context);
            }
            let mut stack = vec![(obj, false)];
            while let Some((id, expanded)) = stack.pop() {
                let Some(h) = store.objects.get(id) else {
                    continue;
                };
                let contexts = h.ext.as_ref().map_or(&[][..], |e| &e.contexts[..]);
                if expanded {
                    if let Some((_, a)) = h.attached.iter().find(|(k, _)| *k == key) {
                        told.push((contexts.first().copied().unwrap_or(h.context), *a));
                    }
                    continue;
                }
                if id != obj && h.flags.contains(ObjectFlags::DESTRUCTION_TOLD) {
                    // Told earlier, with all it owns.
                    continue;
                }
                rooted.extend_from_slice(contexts);
                stack.push((id, true));
                stack.extend(h.children.iter().rev().map(|c| (*c, false)));
            }
            own
        };
        if told.is_empty() {
            return;
        }
        let mut farewells: FxHashMap<ContextId, Farewell> = rooted
            .iter()
            .filter_map(|c| {
                let (parent, serial) = self.context_lineage(*c)?;
                Some((
                    *c,
                    Farewell {
                        parent,
                        serial,
                        ..Farewell::default()
                    },
                ))
            })
            .collect();
        for (c, a) in told {
            // A context that is not going keeps its objects' handlers.
            if let Some(f) = farewells.get_mut(&c) {
                f.attached.push(a);
            }
        }
        let mut tops: Vec<(u64, ContextId)> = Vec::new();
        for c in &rooted {
            let Some((parent, serial)) = farewells.get(c).map(|f| (f.parent, f.serial)) else {
                continue;
            };
            match parent.and_then(|p| farewells.get_mut(&p)) {
                Some(p) => p.inner.push((serial, *c)),
                None => tops.push((serial, *c)),
            }
        }
        tops.sort_unstable_by_key(|a| std::cmp::Reverse(a.0));
        let last = own.len().saturating_sub(1);
        let start = if outermost { last } else { last.min(1) };
        let mut order: Vec<ObjectId> = Vec::new();
        for c in own[start.min(own.len())..]
            .iter()
            .copied()
            .chain(tops.iter().map(|t| t.1))
        {
            let mut stack = vec![c];
            while let Some(c) = stack.pop() {
                let Some(f) = farewells.get_mut(&c) else {
                    continue;
                };
                if std::mem::replace(&mut f.done, true) {
                    continue;
                }
                order.append(&mut f.attached);
                // Popped from the end: the youngest goes on last.
                f.inner.sort_unstable();
                stack.extend(f.inner.iter().map(|i| i.1));
            }
        }
        for a in order {
            if self.is_alive(a) {
                let _ = self.emit_by_name(a, "destruction", &[]);
            }
        }
    }
}

/// `Connections`: handlers for the signals of another object.
#[derive(QmlObject)]
#[qml(module = "QtQml", name = "Connections", extends = QtObject)]
pub struct Connections {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub target: Option<ObjectId>,
    #[qml(notify)]
    pub enabled: bool,
    #[qml(notify)]
    pub ignore_unknown_signals: bool,
    /// `function onFoo(…)` / `onFoo:` by signal name (`foo`).
    #[qml(skip)]
    pub(crate) handlers: Vec<(Atom, Handler)>,
    #[qml(skip)]
    pub(crate) active: Vec<ConnectionId>,
    #[qml(skip)]
    pub(crate) connected_to: Option<ObjectId>,
}

impl Default for Connections {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            target: None,
            enabled: true,
            ignore_unknown_signals: false,
            handlers: Vec::new(),
            active: Vec::new(),
            connected_to: None,
        }
    }
}

impl std::fmt::Debug for Connections {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connections")
            .field("target", &self.target)
            .field("enabled", &self.enabled)
            .field("handlers", &self.handlers.len())
            .finish_non_exhaustive()
    }
}

#[qml_impl]
impl Connections {
    /// Connects from completion on, and again whenever `target` or
    /// `enabled` changes.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        cx.engine
            .effect_in(me, move |e| e.connections_update(me, false))?;
        Ok(())
    }
}

/// `Timer`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQml", name = "Timer", extends = QtObject)]
pub struct Timer {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub interval: i32,
    #[qml(notify)]
    pub running: bool,
    #[qml(notify)]
    pub repeat: bool,
    #[qml(notify)]
    pub triggered_on_start: bool,
    /// The object the timer is declared in. Qt's `Timer` has this property
    /// although a plain `QtObject` has none; the engine keeps it equal to
    /// the owner.
    #[qml(notify, readonly)]
    pub parent: Option<ObjectId>,
}

impl Default for Timer {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            interval: 1000,
            running: false,
            repeat: false,
            triggered_on_start: false,
            parent: None,
        }
    }
}

#[qml_impl]
impl Timer {
    #[qml(signal)]
    fn triggered();

    #[qml(invokable)]
    fn start(cx: &Cx) -> Result<(), EngineError> {
        cx.set(Timer::RUNNING, true)
    }

    #[qml(invokable)]
    fn stop(cx: &Cx) -> Result<(), EngineError> {
        cx.set(Timer::RUNNING, false)
    }

    /// Restarts from now, running or not.
    #[qml(invokable)]
    fn restart(cx: &Cx) -> Result<(), EngineError> {
        if cx.get(Timer::RUNNING)? {
            cx.engine.timer_arm(cx.this, true);
            Ok(())
        } else {
            cx.set(Timer::RUNNING, true)
        }
    }

    /// A `Timer { running: true }` starts once the component is complete,
    /// not while its properties are still being set.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        cx.engine
            .watch(me, Timer::RUNNING.index, move |e, _| e.timer_arm(me, true))?;
        cx.engine.watch(me, Timer::INTERVAL.index, move |e, _| {
            if e.untrack(|| e.get(me, Timer::RUNNING)).unwrap_or(false) {
                e.timer_arm(me, false);
            }
        })?;
        cx.engine.timer_arm(me, true);
        Ok(())
    }
}

/// What a timer entry is for.
enum Pending {
    Timer(ObjectId),
    /// `triggeredOnStart`: Qt posts it, so it fires on the next turn.
    Start(ObjectId),
    Destroy(ObjectId),
    /// A native deadline ([`Engine::call_at`]).
    Call(Box<dyn FnOnce(&Engine)>),
}

/// The engine's timer service.
pub(crate) struct TimerState {
    sched: FrameScheduler,
    clock: Rc<dyn Clock>,
    pending: FxHashMap<TimerId, Pending>,
    by_timer: FxHashMap<ObjectId, TimerId>,
    by_start: FxHashMap<ObjectId, TimerId>,
    /// When each entry of `by_timer` is due.
    due_at: FxHashMap<ObjectId, Time>,
    /// Running timers of frozen objects (§8.3) and when they were due:
    /// armed again on thaw.
    parked: FxHashMap<ObjectId, Time>,
    /// How often each timer fired, counted only while an observer is
    /// attached (`ljos_runtime::observe`).
    fired: FxHashMap<ObjectId, u64>,
    /// `Qt.callLater` queue: one entry per function, with its last args.
    calls: Vec<(JsRef, Vec<JsValue>)>,
    /// When the queue may next run: `None` at once. A call queued while
    /// the queue drains waits a frame, as Qt's waits for the next pass of
    /// the event loop, so a function that queues itself lets frames end.
    calls_due: Option<Time>,
    /// While the queue drains, when it began to.
    draining: Option<Time>,
    quit: bool,
    wake: Option<Rc<dyn Fn()>>,
    /// The pinned wall clock: a UNIX epoch in milliseconds and the value
    /// of `clock` when it was pinned.
    pub(crate) wall: Option<(f64, Time)>,
}

impl Default for TimerState {
    fn default() -> Self {
        Self {
            sched: FrameScheduler::new(),
            clock: Rc::new(SystemClock::new()),
            pending: FxHashMap::default(),
            by_timer: FxHashMap::default(),
            by_start: FxHashMap::default(),
            due_at: FxHashMap::default(),
            parked: FxHashMap::default(),
            fired: FxHashMap::default(),
            calls: Vec::new(),
            calls_due: None,
            draining: None,
            quit: false,
            wake: None,
            wall: None,
        }
    }
}

/// A running Timer as [`Engine::running_timers`] lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunningTimer {
    pub object: ObjectId,
    /// When it next fires; `None` while its object is frozen.
    pub due: Option<Time>,
    pub interval: i32,
    pub repeat: bool,
    /// Fires seen while an observer was attached.
    pub fired: u64,
}

/// Most timer fires one [`Engine::advance_timers`] call catches up on, so a
/// long stall does not replay thousands of ticks.
const MAX_CATCH_UP: u32 = 100;

pub(crate) fn register(engine: &Engine) {
    engine.register_module(crate::module!("QtQml" 2.0 => [
        QtObject, Component, Connections, Timer, crate::instantiator::Instantiator, LocaleEnums
    ]));
    engine.register_module(crate::module!("QtQml.Models" 2.0 => [
        crate::listmodel::ListModel,
        crate::listmodel::ListElement,
        crate::instantiator::Instantiator,
        crate::chooser::DelegateChooser,
        crate::chooser::DelegateChoice,
        crate::sort_filter::SortFilterProxyModel,
        crate::item_selection::ItemSelectionModel,
        crate::sort_filter::FilterBase,
        crate::sort_filter::RoleFilter,
        crate::sort_filter::ValueFilter,
        crate::sort_filter::SorterBase,
        crate::sort_filter::RoleSorter,
        crate::sort_filter::StringSorter
    ]));
    engine.register_module(crate::module!("Qt.labs.qmlmodels" 1.0 => [
        crate::chooser::DelegateChooser,
        crate::chooser::DelegateChoice,
        crate::tree_model::TreeModel,
        crate::table_model::TableModel,
        crate::tree_model::TableModelColumn
    ]));
    engine.register_module(crate::module!("Qt.labs.synchronizer" 1.0 => [
        crate::synchronizer::Synchronizer
    ]));
    // `Synchronizer on value { }`.
    engine.on_value_source(crate::synchronizer::aim);
    // A file that imports `QtQml` has the models too.
    engine.register_import("QtQml", "QtQml.Models");
    // Handlers for another object's signals, and the roles of a row.
    engine.register_open_body(Connections::static_meta());
    engine.register_open_body(crate::listmodel::ListElement::static_meta());
}

/// Drops the timers of destroyed objects.
pub(crate) fn forget_timers(engine: &Engine, objects: &[ObjectId]) {
    let mut st = engine.inner.timers.borrow_mut();
    for o in objects {
        st.parked.remove(o);
        st.due_at.remove(o);
        let ids = [st.by_timer.remove(o), st.by_start.remove(o)];
        for t in ids.into_iter().flatten() {
            st.sched.cancel(t);
            st.pending.remove(&t);
        }
    }
}

/// Takes the timers of frozen objects off the schedule (§8.3): a frozen
/// page's timer wakes nobody.
pub(crate) fn park_timers(engine: &Engine) {
    let armed: Vec<ObjectId> = {
        let st = engine.inner.timers.borrow();
        st.by_timer
            .keys()
            .chain(st.by_start.keys())
            .copied()
            .collect()
    };
    let frozen: Vec<ObjectId> = armed.into_iter().filter(|o| engine.is_frozen(*o)).collect();
    let now = engine.now();
    let mut st = engine.inner.timers.borrow_mut();
    for o in frozen {
        // A posted `triggeredOnStart` is due at once.
        let due = match st.by_start.contains_key(&o) {
            true => now,
            false => st.due_at.get(&o).copied().unwrap_or(now),
        };
        st.due_at.remove(&o);
        let ids = [st.by_timer.remove(&o), st.by_start.remove(&o)];
        for t in ids.into_iter().flatten() {
            st.sched.cancel(t);
            st.pending.remove(&t);
        }
        st.parked.insert(o, due);
    }
}

/// Arms the parked timers that are no longer frozen. One that came due
/// while frozen fires once, at once, and a repeating one goes on from
/// there: observers see that it triggered, not how often (§8.3).
pub(crate) fn unpark_timers(engine: &Engine) {
    let mut parked: Vec<(ObjectId, Time)> = engine
        .inner
        .timers
        .borrow()
        .parked
        .iter()
        .map(|(o, t)| (*o, *t))
        .collect();
    // Timers due together fire in the order they are armed: creation
    // order, not the map's, which changes from run to run.
    parked.sort_unstable_by_key(|p| p.0);
    let now = engine.now();
    for (o, due) in parked {
        if engine.is_frozen(o) {
            continue;
        }
        engine.inner.timers.borrow_mut().parked.remove(&o);
        let Ok((running, interval)) =
            engine.untrack(|| engine.with::<Timer, _>(o, |t| (t.running, t.interval)))
        else {
            continue;
        };
        if running {
            engine.timer_schedule_at(o, due.max(now), interval);
        }
    }
}

impl Engine {
    /// The clock timers read. A [`SystemClock`] unless set.
    pub fn set_clock(&self, clock: Rc<dyn Clock>) {
        self.inner.timers.borrow_mut().clock = clock;
    }

    pub fn now(&self) -> Time {
        self.clock().now()
    }

    /// Pins what JS `Date` and `Date.now()` see: `epoch_ms` (a UNIX epoch,
    /// in milliseconds) is "now" at this call, and time then moves with
    /// the engine's clock, so a virtual clock keeps replays and tests
    /// deterministic. `None` gives back the system's wall clock.
    pub fn set_wall_clock_origin(&self, epoch_ms: Option<f64>) {
        let at = self.now();
        self.inner.timers.borrow_mut().wall = epoch_ms.map(|ms| (ms, at));
        let js = self.inner.js.borrow().clone();
        if let Some(js) = js {
            self.tell_scripts_wall_clock(&js);
        }
    }

    /// The pinned wall clock now, in milliseconds since the UNIX epoch;
    /// `None` while it is not pinned.
    pub fn wall_clock_ms(&self) -> Option<f64> {
        let now = self.now();
        let (origin, at) = self.inner.timers.borrow().wall?;
        Some(origin + now.saturating_since(at).as_secs_f64() * 1000.0)
    }

    /// The clock set by [`Engine::set_clock`], for budgets that must run
    /// on the same time as the timers.
    pub fn clock(&self) -> Rc<dyn Clock> {
        self.inner.timers.borrow().clock.clone()
    }

    /// Called when a timer is armed or a call queued, so the host can
    /// re-arm its OS timer (ask [`Engine::next_timer_deadline`]).
    pub fn set_timer_wake(&self, f: impl Fn() + 'static) {
        self.inner.timers.borrow_mut().wake = Some(Rc::new(f));
    }

    /// The running Timers, for an observer: each with when it is next due,
    /// or `None` while its object is frozen (§8.3). Ordered by object.
    pub fn running_timers(&self) -> Vec<RunningTimer> {
        let st = self.inner.timers.borrow();
        let mut all: Vec<RunningTimer> = st
            .due_at
            .iter()
            .map(|(o, t)| (*o, Some(*t)))
            .chain(st.parked.keys().map(|o| (*o, None)))
            .map(|(object, due)| RunningTimer {
                object,
                due,
                interval: 0,
                repeat: false,
                fired: st.fired.get(&object).copied().unwrap_or(0),
            })
            .collect();
        drop(st);
        for t in &mut all {
            if let Ok((interval, repeat)) =
                self.with::<Timer, _>(t.object, |t| (t.interval, t.repeat))
            {
                t.interval = interval;
                t.repeat = repeat;
            }
        }
        all.sort_by_key(|t| t.object);
        all
    }

    /// When [`Engine::advance_timers`] must next run; `None` while nothing
    /// is scheduled (sleep until input).
    pub fn next_timer_deadline(&self) -> Option<Time> {
        let st = self.inner.timers.borrow();
        // A queued `Qt.callLater` is due when the queue may next run, and
        // only until it ran: a mark on the scheduler would stay, since
        // nothing here starts its frames, and the host would never sleep
        // again.
        let wake = st.sched.next_wake();
        if !st.calls.is_empty() {
            let due = st.calls_due.unwrap_or(Time::ZERO);
            return Some(wake.map_or(due, |w| w.min(due)));
        }
        wake
    }

    /// The display's refresh period (60 Hz until told): how long a
    /// `Qt.callLater` queued while the queue drains waits.
    pub fn set_frame_period(&self, period: Duration) {
        self.inner
            .timers
            .borrow_mut()
            .sched
            .set_frame_period(period);
    }

    /// Whether `Qt.quit()` was called, or [`request_quit`](Self::request_quit).
    pub fn quit_requested(&self) -> bool {
        self.inner.timers.borrow().quit
    }

    /// Asks the host to quit, as `Qt.quit()` does: the request is noted
    /// ([`quit_requested`](Self::quit_requested)) and the host woken, and
    /// the host ends its loop when it next looks. What a Rust application
    /// calls where Qt's calls `QCoreApplication::quit()`.
    pub fn request_quit(&self) {
        self.inner.timers.borrow_mut().quit = true;
        self.wake_host();
    }

    /// Fires every timer due at `now`, in deadline order, then runs the
    /// `Qt.callLater` queue. A repeating timer due several times fires
    /// several times, as if the host had woken for each.
    pub fn advance_timers(&self, now: Time) {
        let mut fired: FxHashMap<ObjectId, u32> = FxHashMap::default();
        loop {
            let next = {
                let mut st = self.inner.timers.borrow_mut();
                let d = st.sched.due(now).next();
                d.and_then(|d| st.pending.remove(&d.id).map(|p| (d, p)))
            };
            let Some((due, what)) = next else { break };
            match what {
                Pending::Destroy(o) => {
                    self.destroy(o);
                }
                Pending::Call(f) => {
                    self.inner.timing.timer_fired();
                    f(self)
                }
                Pending::Start(o) => {
                    self.inner.timing.timer_fired();
                    self.inner.timers.borrow_mut().by_start.remove(&o);
                    let running = self.with::<Timer, _>(o, |t| t.running).unwrap_or(false);
                    if running && let Some(s) = Timer::meta_ref().signal_index("triggered") {
                        let _ = self.emit(o, s, &[]);
                    }
                }
                Pending::Timer(o) => {
                    self.inner.timing.timer_fired();
                    {
                        let mut st = self.inner.timers.borrow_mut();
                        st.by_timer.remove(&o);
                        // Spent unless `timer_fire` arms it again: a single
                        // shot or a stopped timer is no longer due.
                        st.due_at.remove(&o);
                    }
                    let n = fired.entry(o).or_default();
                    *n += 1;
                    let at = if *n >= MAX_CATCH_UP {
                        now
                    } else {
                        due.deadline
                    };
                    self.timer_fire(o, at);
                }
            }
        }
        self.run_pending_calls();
        self.process_deferred_deletes();
    }

    /// Runs the `Qt.callLater` queue if it is due. What the calls queue
    /// runs on the next frame, not in this drain.
    pub fn run_pending_calls(&self) {
        let now = self.now();
        let calls = {
            let mut st = self.inner.timers.borrow_mut();
            if st.draining.is_some() || st.calls_due.is_some_and(|due| due > now) {
                return;
            }
            st.calls_due = None;
            st.draining = Some(now);
            std::mem::take(&mut st.calls)
        };
        for (f, args) in calls {
            self.run_js("Qt.callLater", |s| {
                s.call_ref(&f, &JsValue::Undefined, &args)
            });
        }
        self.inner.timers.borrow_mut().draining = None;
    }

    /// Whether `Qt.callLater` work is queued, due or not.
    pub fn has_pending_calls(&self) -> bool {
        !self.inner.timers.borrow().calls.is_empty()
    }

    /// Whether queued `Qt.callLater` work may run now.
    pub fn has_due_calls(&self) -> bool {
        let now = self.now();
        let st = self.inner.timers.borrow();
        !st.calls.is_empty() && st.draining.is_none() && st.calls_due.is_none_or(|due| due <= now)
    }

    pub(crate) fn call_later(&self, scope: &Scope<'_>, f: JsRef, args: Vec<JsValue>) {
        {
            let mut st = self.inner.timers.borrow_mut();
            match st.calls.iter_mut().find(|(g, _)| scope.same_value(g, &f)) {
                Some(entry) => entry.1 = args,
                None => st.calls.push((f, args)),
            }
            st.calls_due = st.draining.map(|at| at + st.sched.frame_period());
        }
        self.wake_host();
    }

    /// `obj.destroy(delay)`.
    pub(crate) fn destroy_later(&self, obj: ObjectId, delay_ms: Option<f64>) {
        match delay_ms {
            Some(ms) if ms > 0.0 => {
                {
                    let mut st = self.inner.timers.borrow_mut();
                    let at = st.clock.now() + Duration::from_micros((ms * 1000.0) as u64);
                    let id = st.sched.add_timer(at, TimerKind::Timer);
                    st.pending.insert(id, Pending::Destroy(obj));
                }
                self.wake_host();
            }
            _ => {
                self.destroy(obj);
            }
        }
    }

    /// Runs `f` once when the engine's clock reaches `at`, from
    /// [`Engine::advance_timers`]. For native deadlines that must not poll,
    /// such as a media clock's next frame or the end of a stream.
    pub fn call_at(&self, at: Time, f: impl FnOnce(&Engine) + 'static) -> TimerId {
        let id = {
            let mut st = self.inner.timers.borrow_mut();
            let id = st.sched.add_timer(at, TimerKind::Timer);
            st.pending.insert(id, Pending::Call(Box::new(f)));
            id
        };
        self.wake_host();
        id
    }

    /// Drops a [`Engine::call_at`] that has not run yet.
    pub fn cancel_call(&self, id: TimerId) {
        let mut st = self.inner.timers.borrow_mut();
        if matches!(st.pending.get(&id), Some(Pending::Call(_))) {
            st.pending.remove(&id);
            st.sched.cancel(id);
        }
    }

    pub(crate) fn wake_host(&self) {
        // Busy timers are a turn under way, which looks at what is due.
        let wake = self
            .inner
            .timers
            .try_borrow()
            .ok()
            .and_then(|t| t.wake.clone());
        if let Some(w) = wake {
            w();
        }
    }

    /// (Re)starts or stops `obj`'s timer to match `running`. `on_start`:
    /// this is a start, so `triggeredOnStart` applies.
    pub(crate) fn timer_arm(&self, obj: ObjectId, on_start: bool) {
        let Ok((running, interval, on_start_flag)) = self.untrack(|| {
            self.with::<Timer, _>(obj, |t| (t.running, t.interval, t.triggered_on_start))
        }) else {
            return;
        };
        // The engine's own mark, not a field: a field the application
        // cannot see would stop `Timer { .., ..Default::default() }`.
        if !self
            .flags(obj)
            .is_some_and(|f| f.contains(crate::ObjectFlags::COMPLETED))
        {
            return;
        }
        let later = {
            let mut st = self.inner.timers.borrow_mut();
            let before = st.sched.next_wake();
            if let Some(t) = st.by_timer.remove(&obj) {
                st.sched.cancel(t);
                st.pending.remove(&t);
            }
            st.due_at.remove(&obj);
            if !running && let Some(t) = st.by_start.remove(&obj) {
                st.sched.cancel(t);
                st.pending.remove(&t);
            }
            before.is_some() && st.sched.next_wake() != before
        };
        if !running {
            self.inner.timers.borrow_mut().parked.remove(&obj);
            // The host still waits for the deadline that went: told now,
            // it asks again, and does not wake for nothing (a press and
            // hold let go of before its time).
            if later {
                self.wake_host();
            }
            return;
        }
        let now = self.now();
        if self.is_frozen(obj) {
            let posted = on_start && on_start_flag;
            let due = match posted {
                true => now,
                false => now + Duration::from_millis(u64::try_from(interval.max(0)).unwrap_or(0)),
            };
            self.inner.timers.borrow_mut().parked.insert(obj, due);
            return;
        }
        self.inner.timers.borrow_mut().parked.remove(&obj);
        self.timer_schedule(obj, now, interval);
        // Starts within one turn trigger once (checked with qml6).
        let posted = self.inner.timers.borrow().by_start.contains_key(&obj);
        if on_start && on_start_flag && !posted {
            let mut st = self.inner.timers.borrow_mut();
            let id = st
                .sched
                .add_timer(now, TimerKind::for_interval(Duration::ZERO));
            st.pending.insert(id, Pending::Start(obj));
            st.by_start.insert(obj, id);
            drop(st);
            self.wake_host();
        }
    }

    fn timer_schedule(&self, obj: ObjectId, from: Time, interval: i32) {
        let ms = Duration::from_millis(u64::try_from(interval.max(0)).unwrap_or(0));
        self.timer_schedule_at(obj, from + ms, interval);
    }

    fn timer_schedule_at(&self, obj: ObjectId, at: Time, interval: i32) {
        let interval = Duration::from_millis(u64::try_from(interval.max(0)).unwrap_or(0));
        {
            let mut st = self.inner.timers.borrow_mut();
            let id = st.sched.add_timer(at, TimerKind::for_interval(interval));
            st.pending.insert(id, Pending::Timer(obj));
            st.by_timer.insert(obj, id);
            st.due_at.insert(obj, at);
        }
        self.wake_host();
    }

    /// A timer came due at `at`: re-arm (repeat) or stop, then emit.
    fn timer_fire(&self, obj: ObjectId, at: Time) {
        let Ok((running, repeat, interval)) =
            self.with::<Timer, _>(obj, |t| (t.running, t.repeat, t.interval))
        else {
            return;
        };
        if !running {
            return;
        }
        if ljos_runtime::observe::is_enabled() {
            *self.inner.timers.borrow_mut().fired.entry(obj).or_default() += 1;
        }
        if repeat {
            self.timer_schedule(obj, at, interval.max(1));
        } else {
            // The timer stopping itself is not an assignment: a binding on
            // `running` stays.
            let _ = self.write_internal(
                obj,
                Timer::RUNNING.index,
                Value::Bool(false),
                WriteKind::Host,
            );
        }
        if let Some(s) = Timer::meta_ref().signal_index("triggered") {
            let _ = self.emit(obj, s, &[]);
        }
    }

    /// Adds a handler for `signal` of the target to a `Connections`.
    pub(crate) fn connections_add(
        &self,
        obj: ObjectId,
        signal: &str,
        handler: Handler,
    ) -> Result<(), EngineError> {
        self.with_mut::<Connections, _>(obj, |c| c.handlers.push((Atom::new(signal), handler)))?;
        if self
            .flags(obj)
            .is_some_and(|f| f.contains(crate::ObjectFlags::COMPLETED))
        {
            self.untrack(|| self.connections_update(obj, true));
        }
        Ok(())
    }

    /// Reconnects a `Connections` to its current target. Runs in an effect:
    /// reads of `target` and `enabled` are tracked, the rest is not.
    fn connections_update(&self, me: ObjectId, force: bool) {
        let target = self.read(me, Connections::TARGET.index);
        // Enabled changes need not reconnect: the handler checks it when run.
        self.read(me, Connections::ENABLED.index);
        let explicit = self.untrack(|| {
            self.provenance(me, Connections::TARGET.index)
                .is_some_and(|p| p != crate::Provenance::Default)
        });
        self.untrack(|| {
            // Unset, the target is the object the Connections is in.
            let target = match target.as_object() {
                Some(t) => Some(t),
                None if !explicit => self.owner(me),
                None => None,
            };
            let target = target.filter(|t| self.is_alive(*t));
            let Ok((current, handlers, ignore)) = self.with::<Connections, _>(me, |c| {
                (c.connected_to, c.handlers.clone(), c.ignore_unknown_signals)
            }) else {
                return;
            };
            if !force && current == target {
                return;
            }
            let Ok(old) = self.with_mut::<Connections, _>(me, |c| {
                c.connected_to = target;
                std::mem::take(&mut c.active)
            }) else {
                return;
            };
            for id in old {
                self.disconnect(id);
            }
            let Some(target) = target else {
                return;
            };
            let mut active = Vec::new();
            for (name, h) in handlers {
                match self.signal_index(target, &name) {
                    Ok(s) => {
                        let h = h.clone();
                        let guarded: Handler = Rc::new(move |e, args| {
                            if e.read(me, Connections::ENABLED.index).to_boolean() {
                                h(e, args);
                            }
                        });
                        let later = crate::signal::Order::Later;
                        if let Ok(id) =
                            self.connect_ordered(target, s, Some(me), guarded, later)
                        {
                            active.push(id);
                        }
                    }
                    Err(_) if ignore => {}
                    Err(_) => {
                        let on = handler_name(&name);
                        self.warn(format!(
                            "QML Connections: Detected function \"{on}\" in Connections element. This is probably intended to be a function declaration."
                        ));
                    }
                }
            }
            let _ = self.with_mut::<Connections, _>(me, |c| c.active = active);
        });
    }

    /// Makes an object from the `Component` `component`'s template.
    pub fn component_create(
        &self,
        component: ObjectId,
        parent: Option<ObjectId>,
        props: Vec<(String, Value)>,
    ) -> Result<ObjectId, EngineError> {
        let template = self.component_template(component)?;
        self.as_host_root(|| self.create_from_template(&template, parent, props))
    }

    /// What a `createObject` of `component` left it in: Error with
    /// `failed` for `errorString()` when it could not set a required
    /// property, Ready again once one succeeds. Qt changes the status
    /// without saying so, and so does this.
    fn component_made(&self, component: ObjectId, failed: Option<&str>) {
        let _ = self.with_mut::<Component, _>(component, |c| match failed {
            Some(why) => {
                c.status = 3;
                c.error = Atom::new(&format!("{why}\n"));
            }
            None if c.status == 3 && c.template.is_some() => {
                c.status = 1;
                c.error = Atom::default();
            }
            None => {}
        });
    }

    /// Whether `component` is in Error ([`Component::status`]).
    pub(crate) fn component_failed(&self, component: ObjectId) -> bool {
        self.with::<Component, _>(component, |c| c.status == 3)
            .unwrap_or(false)
    }

    pub(crate) fn component_template(
        &self,
        component: ObjectId,
    ) -> Result<Rc<Template>, EngineError> {
        let template = self
            .with::<Component, _>(component, |c| c.template.clone())?
            .ok_or_else(|| EngineError::Load("the component is not ready".into()))?;
        let fresh = self.fresh_template(&template);
        if !Rc::ptr_eq(&fresh, &template) {
            self.with_mut::<Component, _>(component, |c| c.template = Some(fresh.clone()))?;
        }
        Ok(fresh)
    }

    /// The entries of a JS object (`createObject`'s properties); none for
    /// any other value.
    pub fn js_object_entries(&self, v: &Value) -> Vec<(String, Value)> {
        let Value::Js(h) = v else {
            return Vec::new();
        };
        let r = h.0.clone();
        self.run_js("properties", |s| {
            let entries = s.entries(&r)?;
            Ok(entries
                .into_iter()
                .map(|(k, v)| (k, self.js_to_value(s, v, &crate::TypeTag::Var)))
                .collect())
        })
        .unwrap_or_default()
    }

    /// One member of a JS object (`source` of a regular expression);
    /// `None` for a value that is not a JS object and for a member it
    /// does not have.
    pub fn js_member(&self, v: &Value, key: &str) -> Option<Value> {
        let Value::Js(h) = v else {
            return None;
        };
        let r = h.0.clone();
        let out = self.run_js("member", |s| {
            let field = s.get_field(&r, key)?;
            Ok(self.js_to_value(s, field, &crate::TypeTag::Var))
        })?;
        (!matches!(out, Value::Undefined)).then_some(out)
    }

    /// `String(v)` for a JS object, as a script prints it: a Date's time,
    /// an array's items. `None` for any other value.
    pub fn js_text(&self, v: &Value) -> Option<String> {
        let Value::Js(h) = v else {
            return None;
        };
        let r = JsValue::Handle(h.0.clone());
        self.try_run_js("text", |s| Ok(s.display(&r))).ok()
    }

    /// Whether `v` is a JS function, as `QJSValue::isCallable` tells: what
    /// [`Engine::call_js`] can call. A plain object, a number or a QML
    /// object is not.
    pub fn is_callable(&self, v: &Value) -> bool {
        let Value::Js(h) = v else {
            return false;
        };
        let r = h.0.clone();
        self.try_run_js("is_callable", |s| Ok(s.is_function(&r)))
            .unwrap_or(false)
    }

    /// What a binding is warned when its value `v` is refused, as qml6
    /// words it: a function is what only a var takes, and a Date is named
    /// by its C++ type.
    pub(crate) fn refusal(&self, v: &Value, e: &crate::EngineError) -> String {
        if let (Value::Js(h), crate::EngineError::Convert(c)) = (v, e)
            && c.at.is_none()
        {
            let r = h.0.clone();
            match self.try_run_js("refusal", |s| Ok((s.is_function(&r), s.is_date(&r)))) {
                Ok((true, _)) => {
                    return "Unable to assign a function to a property of any type other than var."
                        .into();
                }
                Ok((_, true)) => {
                    return crate::types::ConvertError::new("QDateTime", c.to.clone()).to_string();
                }
                _ => {}
            }
        }
        e.to_string()
    }

    /// Calls a JS function a property holds (`SpinBox.textFromValue`).
    /// `None` when `f` is not a function or it threw; the exception is
    /// logged as Qt logs it. [`Engine::try_call_js`] gives the error back
    /// instead.
    pub fn call_js(&self, f: &Value, args: &[Value]) -> Option<Value> {
        let Value::Js(h) = f else {
            return None;
        };
        let r = h.0.clone();
        let args: Vec<JsValue> = args.iter().map(|a| self.to_js(a)).collect();
        self.run_js("call", |s| {
            let out = s.call_ref(&r, &JsValue::Undefined, &args)?;
            Ok(self.js_to_value(s, out, &crate::TypeTag::Var))
        })
    }

    /// [`Engine::call_js`] for a caller that reports the failure itself:
    /// the thrown error (its message, where it was thrown and the QML
    /// trace) or [`JsError::NotCallable`] comes back, and nothing is
    /// logged.
    pub fn try_call_js(&self, f: &Value, args: &[Value]) -> Result<Value, JsError> {
        self.try_call(&JsValue::Undefined, f, args)
    }

    /// [`Engine::call_value`] for a caller that reports the failure itself,
    /// as [`Engine::try_call_js`].
    pub fn try_call_value(
        &self,
        this: ObjectId,
        func: &Value,
        args: &[Value],
    ) -> Result<Value, JsError> {
        self.try_call(&JsValue::Object(crate::js::obj_to_u64(this)), func, args)
    }

    fn try_call(&self, this: &JsValue, f: &Value, args: &[Value]) -> Result<Value, JsError> {
        let Value::Js(h) = f else {
            return Err(JsError::NotCallable);
        };
        if !self.is_callable(f) {
            return Err(JsError::NotCallable);
        }
        let r = h.0.clone();
        let args: Vec<JsValue> = args.iter().map(|a| self.to_js(a)).collect();
        self.try_run_js("call", |s| {
            let out = s.call_ref(&r, this, &args)?;
            Ok(self.js_to_value(s, out, &crate::TypeTag::Var))
        })
    }

    /// Calls a JS function a property holds with `this` bound to `this`
    /// (`CheckBox.nextCheckState`). `None` when it is not a function or it
    /// threw, logged as [`Engine::call_js`] logs it;
    /// [`Engine::try_call_value`] gives the error back instead.
    pub fn call_value(&self, this: ObjectId, func: &Value, args: &[Value]) -> Option<Value> {
        let Value::Js(h) = func else {
            return None;
        };
        let r = h.0.clone();
        let this_js = JsValue::Object(crate::js::obj_to_u64(this));
        let js_args: Vec<JsValue> = args.iter().map(|a| self.to_js(a)).collect();
        self.run_js("callback", |s| {
            let out = s.call_ref(&r, &this_js, &js_args)?;
            Ok(self.js_to_value(s, out, &crate::TypeTag::Var))
        })
    }
}

/// `foo` → `onFoo`.
pub fn handler_name(signal: &str) -> String {
    let mut c = signal.chars();
    match c.next() {
        Some(f) => format!("on{}{}", f.to_uppercase(), c.as_str()),
        None => "on".into(),
    }
}

/// The meta of a native type, as a [`crate::MetaRef`].
trait MetaRefOf {
    fn meta_ref() -> crate::MetaRef;
}

impl<T: QmlType> MetaRefOf for T {
    fn meta_ref() -> crate::MetaRef {
        crate::MetaRef::Static(T::static_meta())
    }
}
