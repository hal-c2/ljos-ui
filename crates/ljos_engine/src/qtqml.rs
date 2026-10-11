//! The QtQml module's non-visual types (contract items 3 and 4):
//! `Component`, `Connections` and `Timer`, plus the engine's timer service
//! (`Timer`, `Qt.callLater`, `destroy(delay)`).
//!
//! Timers live on a [`FrameScheduler`] read against a [`Clock`]. Nothing
//! polls: the host asks [`Engine::next_timer_deadline`] when to wake and
//! calls [`Engine::advance_timers`] then; tests use a `VirtualClock` and
//! call it directly.

use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use ljos_js::{JsRef, JsValue, Scope};
use ljos_runtime::{Clock, FrameScheduler, SystemClock, Time, TimerId, TimerKind, WakeReasons};

use crate::atom::Atom;
use crate::builtins::QtObject;
use crate::cx::Cx;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{ConnectionId, ContextId, ObjectId};
use crate::incubator::Template;
use crate::meta::MetaRef;
use crate::object::{ObjectFlags, QmlType};
use crate::signal::Handler;
use crate::value::Value;
use crate::{QmlObject, WriteKind, qml_impl};

/// `Component.status` values.
fn component_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Status",
        vec![("Null", 0), ("Ready", 1), ("Loading", 2), ("Error", 3)],
    )]
}

/// `Component`: a template objects are made from (`Component { … }`, an
/// inline `component`, a property of type `Component`).
#[derive(Default, QmlObject)]
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
    #[qml(notify, readonly)]
    pub status: i32,
    #[qml(readonly)]
    pub url: Atom,
    #[qml(skip)]
    pub(crate) template: Option<Rc<Template>>,
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
        let props = cx.engine.js_object_entries(&props);
        match cx.engine.component_create(cx.this, parent, props) {
            Ok(o) => Value::Object(o),
            Err(e) => {
                cx.engine.warn(format!("Component.createObject: {e}"));
                Value::Null
            }
        }
    }

    #[qml(invokable, name = "errorString")]
    fn error_string(&self) -> Atom {
        Atom::default()
    }
}

/// What `Component.` attaches: `onCompleted`, `onDestruction`.
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
    pub(crate) fn tell_destruction(&self, obj: ObjectId, outermost: bool) {
        if !self.inner.destruction_handlers.get() {
            return;
        }
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
        let mut farewells: HashMap<ContextId, Farewell> = rooted
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
        cx.engine.effect_in(me, move |e| e.connections_update(me))?;
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
    #[qml(skip)]
    completed: bool,
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
            completed: false,
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
        cx.with_mut::<Timer, _>(|t| t.completed = true)?;
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
}

/// The engine's timer service.
pub(crate) struct TimerState {
    sched: FrameScheduler,
    clock: Rc<dyn Clock>,
    pending: HashMap<TimerId, Pending>,
    by_timer: HashMap<ObjectId, TimerId>,
    by_start: HashMap<ObjectId, TimerId>,
    /// `Qt.callLater` queue: one entry per function, with its last args.
    calls: Vec<(JsRef, Vec<JsValue>)>,
    quit: bool,
    wake: Option<Rc<dyn Fn()>>,
}

impl Default for TimerState {
    fn default() -> Self {
        Self {
            sched: FrameScheduler::new(),
            clock: Rc::new(SystemClock::new()),
            pending: HashMap::new(),
            by_timer: HashMap::new(),
            by_start: HashMap::new(),
            calls: Vec::new(),
            quit: false,
            wake: None,
        }
    }
}

/// Most timer fires one [`Engine::advance_timers`] call catches up on, so a
/// long stall does not replay thousands of ticks.
const MAX_CATCH_UP: u32 = 100;

pub(crate) fn register(engine: &Engine) {
    engine.register_module(crate::module!("QtQml" 2.0 => [
        QtObject, Component, Connections, Timer, crate::instantiator::Instantiator
    ]));
    engine.register_module(crate::module!("QtQml.Models" 2.0 => [
        crate::listmodel::ListModel, crate::listmodel::ListElement
    ]));
}

/// Drops the timers of destroyed objects.
pub(crate) fn forget_timers(engine: &Engine, objects: &[ObjectId]) {
    let mut st = engine.inner.timers.borrow_mut();
    for o in objects {
        let ids = [st.by_timer.remove(o), st.by_start.remove(o)];
        for t in ids.into_iter().flatten() {
            st.sched.cancel(t);
            st.pending.remove(&t);
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

    /// When [`Engine::advance_timers`] must next run; `None` while nothing
    /// is scheduled (sleep until input).
    pub fn next_timer_deadline(&self) -> Option<Time> {
        self.inner.timers.borrow().sched.next_wake()
    }

    /// Whether `Qt.quit()` was called.
    pub fn quit_requested(&self) -> bool {
        self.inner.timers.borrow().quit
    }

    pub(crate) fn request_quit(&self) {
        self.inner.timers.borrow_mut().quit = true;
        self.wake_host();
    }

    /// Fires every timer due at `now`, in deadline order, then runs the
    /// `Qt.callLater` queue. A repeating timer due several times fires
    /// several times, as if the host had woken for each.
    pub fn advance_timers(&self, now: Time) {
        let mut fired: HashMap<ObjectId, u32> = HashMap::new();
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
                Pending::Start(o) => {
                    self.inner.timers.borrow_mut().by_start.remove(&o);
                    let running = self.with::<Timer, _>(o, |t| t.running).unwrap_or(false);
                    if running && let Some(s) = Timer::meta_ref().signal_index("triggered") {
                        let _ = self.emit(o, s, &[]);
                    }
                }
                Pending::Timer(o) => {
                    self.inner.timers.borrow_mut().by_timer.remove(&o);
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

    /// Runs the `Qt.callLater` queue.
    pub fn run_pending_calls(&self) {
        loop {
            let calls = std::mem::take(&mut self.inner.timers.borrow_mut().calls);
            if calls.is_empty() {
                break;
            }
            for (f, args) in calls {
                self.run_js("Qt.callLater", |s| {
                    s.call_ref(&f, &JsValue::Undefined, &args)
                });
            }
        }
    }

    /// Whether `Qt.callLater` work is queued.
    pub fn has_pending_calls(&self) -> bool {
        !self.inner.timers.borrow().calls.is_empty()
    }

    pub(crate) fn call_later(&self, scope: &Scope<'_>, f: JsRef, args: Vec<JsValue>) {
        {
            let mut st = self.inner.timers.borrow_mut();
            match st.calls.iter_mut().find(|(g, _)| scope.same_value(g, &f)) {
                Some(entry) => entry.1 = args,
                None => st.calls.push((f, args)),
            }
            st.sched.mark(WakeReasons::DIRTY);
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

    pub(crate) fn wake_host(&self) {
        let wake = self.inner.timers.borrow().wake.clone();
        if let Some(w) = wake {
            w();
        }
    }

    /// (Re)starts or stops `obj`'s timer to match `running`. `on_start`:
    /// this is a start, so `triggeredOnStart` applies.
    pub(crate) fn timer_arm(&self, obj: ObjectId, on_start: bool) {
        let Ok((running, interval, on_start_flag, completed)) = self.untrack(|| {
            self.with::<Timer, _>(obj, |t| {
                (t.running, t.interval, t.triggered_on_start, t.completed)
            })
        }) else {
            return;
        };
        if !completed {
            return;
        }
        {
            let mut st = self.inner.timers.borrow_mut();
            if let Some(t) = st.by_timer.remove(&obj) {
                st.sched.cancel(t);
                st.pending.remove(&t);
            }
            if !running && let Some(t) = st.by_start.remove(&obj) {
                st.sched.cancel(t);
                st.pending.remove(&t);
            }
        }
        if !running {
            return;
        }
        let now = self.now();
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
        let interval = Duration::from_millis(u64::try_from(interval.max(0)).unwrap_or(0));
        {
            let mut st = self.inner.timers.borrow_mut();
            let id = st
                .sched
                .add_timer(from + interval, TimerKind::for_interval(interval));
            st.pending.insert(id, Pending::Timer(obj));
            st.by_timer.insert(obj, id);
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
        if repeat {
            self.timer_schedule(obj, at, interval.max(1));
        } else {
            // The timer stopping itself is not an assignment: a binding on
            // `running` stays.
            let _ = self.write_internal(
                obj,
                Timer::RUNNING.index,
                Value::Bool(false),
                WriteKind::Animation,
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
            self.untrack(|| self.connections_update(obj));
        }
        Ok(())
    }

    /// Reconnects a `Connections` to its current target. Runs in an effect:
    /// reads of `target` and `enabled` are tracked, the rest is not.
    fn connections_update(&self, me: ObjectId) {
        let target = self.read(me, Connections::TARGET.index);
        let enabled = self.read(me, Connections::ENABLED.index).to_boolean();
        let explicit = self.untrack(|| {
            self.provenance(me, Connections::TARGET.index)
                .is_some_and(|p| p != crate::Provenance::Default)
        });
        self.untrack(|| {
            let Ok((old, handlers, ignore)) = self.with_mut::<Connections, _>(me, |c| {
                (
                    std::mem::take(&mut c.active),
                    c.handlers.clone(),
                    c.ignore_unknown_signals,
                )
            }) else {
                return;
            };
            for id in old {
                self.disconnect(id);
            }
            // Unset, the target is the object the Connections is in.
            let target = match target.as_object() {
                Some(t) => Some(t),
                None if !explicit => self.owner(me),
                None => None,
            };
            let Some(target) = target.filter(|t| self.is_alive(*t)) else {
                return;
            };
            if !enabled {
                return;
            }
            let mut active = Vec::new();
            for (name, h) in handlers {
                match self.signal_index(target, &name) {
                    Ok(s) => {
                        let h = h.clone();
                        if let Ok(id) = self.connect_to(target, s, me, move |e, a| h(e, a)) {
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
        self.create_from_template(&template, parent, props)
    }

    pub(crate) fn component_template(
        &self,
        component: ObjectId,
    ) -> Result<Rc<Template>, EngineError> {
        self.with::<Component, _>(component, |c| c.template.clone())?
            .ok_or_else(|| EngineError::Load("the component is not ready".into()))
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

    /// Calls a JS function a property holds (`SpinBox.textFromValue`).
    /// `None` when `f` is not a function or it threw; the exception is
    /// logged as Qt logs it.
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
}

/// `foo` → `onFoo`.
pub(crate) fn handler_name(signal: &str) -> String {
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
