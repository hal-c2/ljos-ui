//! Faults reach one place: what is caught where, what the engine is
//! worth afterwards, and what a boundary does with a fault below it.

mod support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use ljos_engine::{
    Boundary, BoundaryConfig, Cx, Engine, FaultReport, ObjectId, QmlObject, QtObject, Value,
    fault_kind, qml_impl,
};
use ljos_runtime::{Fault, Intensity, JobPoll, Restart};
use support::{Harness, engine};

/// A native type whose methods fail the way a bug in one would.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Faults", name = "Bomb", extends = QtObject)]
struct Bomb {
    #[qml(base)]
    base: QtObject,
    #[qml(notify)]
    armed: bool,
}

#[qml_impl]
impl Bomb {
    #[qml(signal)]
    fn tick();

    #[qml(invokable)]
    fn explode(&self) {
        panic!("boom");
    }

    /// Panics after it has written: what a panic leaves half done.
    #[qml(invokable, name = "armThenExplode")]
    fn arm_then_explode(cx: &Cx) {
        cx.set(Bomb::ARMED, true).unwrap();
        panic!("armed and gone");
    }

    #[qml(invokable)]
    fn fine(&self) -> f64 {
        7.0
    }
}

/// A boundary whose content is one `Bomb`.
#[derive(Debug, QmlObject)]
#[qml(module = "Faults", name = "Guard", extends = QtObject)]
struct Guard {
    #[qml(base)]
    base: QtObject,
    #[qml(skip)]
    restart: Restart,
    #[qml(skip)]
    max_restarts: u32,
    #[qml(skip)]
    catch_exceptions: bool,
    /// The next start fails with this.
    #[qml(skip)]
    refuse: Option<String>,
}

impl Default for Guard {
    fn default() -> Self {
        Self {
            base: QtObject::default(),
            restart: Restart::Permanent,
            max_restarts: 2,
            catch_exceptions: false,
            refuse: None,
        }
    }
}

type Events = Rc<RefCell<Vec<String>>>;

struct Recorder(Events);

fn name(e: &Engine, o: ObjectId) -> String {
    e.read_by_name(o, "objectName").to_js_string().to_string()
}

impl Boundary for Recorder {
    fn config(&self, e: &Engine, me: ObjectId) -> BoundaryConfig {
        e.with(me, |g: &Guard| BoundaryConfig {
            restart: g.restart,
            intensity: Intensity::new(g.max_restarts, Duration::from_secs(1)),
            catch_exceptions: g.catch_exceptions,
        })
        .unwrap()
    }

    fn stop(&self, e: &Engine, me: ObjectId, report: &FaultReport) {
        self.0.borrow_mut().push(format!(
            "stop {} {}: {}",
            name(e, me),
            report.kind(),
            report.message()
        ));
        for c in e.children(me) {
            e.destroy_now(c);
        }
    }

    fn start(&self, e: &Engine, me: ObjectId) -> Result<(), Fault> {
        if let Some(why) = e.with_mut(me, |g: &mut Guard| g.refuse.take()).unwrap() {
            self.0.borrow_mut().push(format!("refuse {}", name(e, me)));
            return Err(Fault::Startup(why));
        }
        self.0.borrow_mut().push(format!("start {}", name(e, me)));
        e.create_native(Bomb::default(), Some(me)).unwrap();
        Ok(())
    }
}

/// An engine with the boundary type registered, and what its boundaries
/// did.
fn guarded() -> (Harness, Events) {
    let h = engine();
    let events = Events::default();
    h.engine
        .register_boundary::<Guard>(Rc::new(Recorder(events.clone())));
    (h, events)
}

fn guard(e: &Engine, owner: Option<ObjectId>, name: &str, f: impl FnOnce(&mut Guard)) -> ObjectId {
    let mut g = Guard::default();
    f(&mut g);
    let o = e.create_native(g, owner).unwrap();
    e.write_by_name(o, "objectName", Value::from(name)).unwrap();
    o
}

fn bomb(e: &Engine, owner: Option<ObjectId>) -> ObjectId {
    e.create_native(Bomb::default(), owner).unwrap()
}

/// The bomb `guard` holds now.
fn content(e: &Engine, guard: ObjectId) -> ObjectId {
    *e.children(guard).first().expect("the guard has content")
}

fn explode(e: &Engine, bomb: ObjectId) {
    assert!(e.invoke_by_name(bomb, "explode", &[]).is_err());
}

fn taken(events: &Events) -> Vec<String> {
    std::mem::take(&mut *events.borrow_mut())
}

// ------------------------------------------------------------------ panics

#[test]
fn a_panic_in_a_native_method_is_an_error_and_a_fault() {
    let h = engine();
    let e = &h.engine;
    let b = bomb(e, None);
    assert!(e.invoke_by_name(b, "explode", &[]).is_err());
    assert!(e.has_pending_faults());
    assert!(e.application_fault().is_none(), "not before delivery");
    assert_eq!(e.deliver_faults(), 1);
    assert!(!e.has_pending_faults());
    let fault = e.application_fault().expect("no boundary took it");
    assert_eq!(fault.kind(), "Panic");
    assert_eq!(fault.message(), "boom");
    assert_eq!(fault.origin, Some(b));
    // The engine and the object are as usable as before.
    assert_eq!(e.invoke_by_name(b, "fine", &[]).unwrap(), Value::from(7.0));
    e.clear_application_fault();
    assert!(e.application_fault().is_none());
}

#[test]
fn what_a_method_wrote_before_it_panicked_stays_written() {
    let h = engine();
    let e = &h.engine;
    let b = bomb(e, None);
    let seen = Rc::new(Cell::new(0));
    let s = seen.clone();
    let armed = e.property_index(b, "armed").unwrap();
    e.watch(b, armed, move |_, _| s.set(s.get() + 1)).unwrap();
    assert!(e.invoke_by_name(b, "armThenExplode", &[]).is_err());
    assert_eq!(e.read_by_name(b, "armed"), Value::from(true));
    assert_eq!(seen.get(), 1, "the write was notified");
    // No batch or borrow is left held: the next write goes through.
    e.write_by_name(b, "armed", Value::from(false)).unwrap();
    assert_eq!(seen.get(), 2);
}

#[test]
fn a_panic_in_one_handler_does_not_stop_the_next() {
    let h = engine();
    let e = &h.engine;
    let b = bomb(e, None);
    let tick = e.signal_index(b, "tick").unwrap();
    let ran = Rc::new(Cell::new(0));
    e.connect(b, tick, |_, _| panic!("first handler")).unwrap();
    let r = ran.clone();
    e.connect(b, tick, move |_, _| r.set(r.get() + 1)).unwrap();
    e.emit(b, tick, &[]).unwrap();
    assert_eq!(ran.get(), 1);
    e.emit(b, tick, &[]).unwrap();
    assert_eq!(ran.get(), 2);
    let faults = e.faults();
    assert_eq!(faults.len(), 2);
    assert!(faults.iter().all(|f| f.message() == "first handler"));
}

#[test]
fn a_turn_catches_a_panic_and_names_the_object() {
    let h = engine();
    let e = &h.engine;
    let b = bomb(e, None);
    assert_eq!(e.turn(None, || 3), Some(3));
    assert!(e.faults().is_empty());
    let r: Option<i32> = e.turn(Some(b), || panic!("in a turn"));
    assert_eq!(r, None);
    let faults = e.faults();
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0].origin, Some(b));
    assert_eq!(faults[0].message(), "in a turn");
    // A payload that is not text is still a panic.
    let r: Option<()> = e.turn(None, || std::panic::panic_any(42_i32));
    assert_eq!(r, None);
    assert_eq!(e.faults()[1].kind(), "Panic");
}

#[test]
fn the_object_that_panicked_is_the_origin_whoever_caught_it() {
    let h = engine();
    let e = &h.engine;
    let outer = bomb(e, None);
    let inner = bomb(e, None);
    let tick = e.signal_index(outer, "tick").unwrap();
    // A handler that `inner` receives: its code is what fails.
    e.connect_to(outer, tick, inner, |_, _| panic!("inner's handler"))
        .unwrap();
    let r = e.turn(Some(outer), || e.emit(outer, tick, &[]));
    assert!(r.is_some(), "the handler's panic ended only the handler");
    let faults = e.faults();
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0].origin, Some(inner));
}

#[test]
fn the_host_is_told_at_delivery() {
    let h = engine();
    let e = &h.engine;
    let b = bomb(e, None);
    let told: Rc<RefCell<Vec<String>>> = Rc::default();
    let t = told.clone();
    e.set_fault_handler(move |_, f| {
        t.borrow_mut()
            .push(format!("{}: {}", f.kind(), f.message()))
    });
    let woke = Rc::new(Cell::new(0));
    let w = woke.clone();
    e.set_timer_wake(move || w.set(w.get() + 1));
    explode(e, b);
    assert!(told.borrow().is_empty(), "nothing is handled mid-turn");
    assert_eq!(woke.get(), 1, "the host is asked for a turn");
    e.deliver_faults();
    assert_eq!(*told.borrow(), ["Panic: boom"]);
    assert_eq!(e.deliver_faults(), 0, "nothing is left to deliver");
}

#[test]
fn every_kind_has_a_name_and_the_history_is_bounded() {
    let h = engine();
    let e = &h.engine;
    assert_eq!(fault_kind(&Fault::JsOutOfMemory), "JsOutOfMemory");
    assert_eq!(fault_kind(&Fault::Startup("x".into())), "Startup");
    assert_eq!(fault_kind(&Fault::Panic("x".into())), "Panic");
    for i in 0..40 {
        e.report_fault(None, Fault::Startup(format!("{i}")));
    }
    let faults = e.faults();
    assert_eq!(faults.len(), 32);
    assert_eq!(faults[0].message(), "8");
    assert_eq!(faults[31].message(), "39");
    assert_eq!(e.deliver_faults(), 40);
    assert_eq!(e.application_fault().unwrap().message(), "39");
}

// -------------------------------------------------------------- boundaries

#[test]
fn a_fault_below_a_boundary_restarts_its_content() {
    let (h, events) = guarded();
    let e = &h.engine;
    let neighbour = bomb(e, None);
    let g = guard(e, None, "g", |_| {});
    let first = bomb(e, Some(g));
    assert_eq!(e.boundary_of(first), Some(g));
    assert_eq!(e.boundary_of(neighbour), None);
    assert!(e.supervisor_dump().is_none(), "no tree before a fault");

    explode(e, first);
    assert!(e.is_alive(first), "nothing is destroyed where it is caught");
    assert!(taken(&events).is_empty());
    assert_eq!(e.deliver_faults(), 1);
    assert_eq!(taken(&events), ["stop g Panic: boom", "start g"]);
    assert!(!e.is_alive(first));
    assert!(e.is_alive(neighbour));
    assert!(e.application_fault().is_none(), "the boundary took it");
    assert_eq!(e.boundary_recent_restarts(g), 1);
    let second = content(e, g);
    assert_ne!(second, first);
    assert_eq!(
        e.invoke_by_name(second, "fine", &[]).unwrap(),
        Value::from(7.0)
    );
    let dump = e.supervisor_dump().unwrap();
    assert!(dump.contains("boundary"), "{dump}");
}

#[test]
fn more_restarts_than_the_intensity_allows_reach_the_application() {
    let (h, events) = guarded();
    let e = &h.engine;
    let g = guard(e, None, "g", |g| g.max_restarts = 2);
    bomb(e, Some(g));
    for _ in 0..2 {
        explode(e, content(e, g));
        e.deliver_faults();
        assert_eq!(taken(&events), ["stop g Panic: boom", "start g"]);
        h.clock.advance(Duration::from_millis(100));
    }
    assert!(e.application_fault().is_none());
    explode(e, content(e, g));
    e.deliver_faults();
    assert_eq!(
        taken(&events),
        ["stop g Panic: boom"],
        "stopped and not started again"
    );
    let fault = e.application_fault().expect("the boundary gave up");
    assert_eq!(fault.kind(), "Panic");
    assert!(e.children(g).is_empty());
}

#[test]
fn restarts_outside_the_window_do_not_count() {
    let (h, events) = guarded();
    let e = &h.engine;
    let g = guard(e, None, "g", |g| g.max_restarts = 1);
    bomb(e, Some(g));
    for _ in 0..5 {
        explode(e, content(e, g));
        e.deliver_faults();
        assert_eq!(taken(&events), ["stop g Panic: boom", "start g"]);
        // The window is one second of the engine's clock.
        h.clock.advance(Duration::from_millis(1500));
    }
    assert!(e.application_fault().is_none());
}

#[test]
fn a_boundary_that_gives_up_hands_the_fault_to_the_one_around_it() {
    let (h, events) = guarded();
    let e = &h.engine;
    let outer = guard(e, None, "outer", |g| g.max_restarts = 3);
    let inner = guard(e, Some(outer), "inner", |g| g.max_restarts = 0);
    let b = bomb(e, Some(inner));
    assert_eq!(e.boundary_of(b), Some(inner));
    assert_eq!(e.boundary_of(inner), Some(outer));
    explode(e, b);
    e.deliver_faults();
    assert_eq!(
        taken(&events),
        [
            "stop inner Panic: boom",
            "stop outer Panic: boom",
            "start outer"
        ]
    );
    assert!(
        !e.is_alive(inner),
        "the outer content went, the inner boundary with it"
    );
    assert!(e.application_fault().is_none());
    assert_eq!(e.boundary_recent_restarts(outer), 1);
    // The outer boundary's new content faults like the first.
    explode(e, content(e, outer));
    e.deliver_faults();
    assert_eq!(taken(&events), ["stop outer Panic: boom", "start outer"]);
}

#[test]
fn temporary_content_stays_stopped() {
    let (h, events) = guarded();
    let e = &h.engine;
    let g = guard(e, None, "g", |g| g.restart = Restart::Temporary);
    let b = bomb(e, Some(g));
    explode(e, b);
    e.deliver_faults();
    assert_eq!(taken(&events), ["stop g Panic: boom"]);
    assert!(e.application_fault().is_none(), "stopped is handled");
    // Content its owner makes later is supervised again.
    let again = bomb(e, Some(g));
    explode(e, again);
    e.deliver_faults();
    assert_eq!(taken(&events), ["stop g Panic: boom"]);
    assert!(e.application_fault().is_none());
}

#[test]
fn a_start_that_fails_is_one_more_exit() {
    let (h, events) = guarded();
    let e = &h.engine;
    let g = guard(e, None, "g", |g| {
        g.max_restarts = 2;
        g.refuse = Some("no content".into());
    });
    let b = bomb(e, Some(g));
    explode(e, b);
    e.deliver_faults();
    // The second attempt is within the intensity and succeeds.
    assert_eq!(
        taken(&events),
        ["stop g Panic: boom", "refuse g", "start g"]
    );
    assert!(e.application_fault().is_none());
    assert_eq!(e.boundary_recent_restarts(g), 2);
}

#[test]
fn two_faults_of_one_subtree_restart_it_once() {
    let (h, events) = guarded();
    let e = &h.engine;
    let g = guard(e, None, "g", |_| {});
    let b = bomb(e, Some(g));
    explode(e, b);
    explode(e, b);
    assert_eq!(e.deliver_faults(), 2);
    assert_eq!(
        taken(&events),
        ["stop g Panic: boom", "start g"],
        "the second fault is of content that is gone"
    );
    assert_eq!(e.boundary_recent_restarts(g), 1);
}

#[test]
fn work_is_charged_to_the_boundary_around_an_object() {
    let (h, _events) = guarded();
    let e = &h.engine;
    let loose = bomb(e, None);
    let g = guard(e, None, "g", |_| {});
    let b = bomb(e, Some(g));
    let other = guard(e, None, "other", |_| {});
    assert_eq!(e.proc_for(loose), ljos_engine::APPLICATION_PROC);
    let proc = e.proc_for(b);
    assert_ne!(proc, ljos_engine::APPLICATION_PROC);
    assert_eq!(e.proc_for(g), proc);
    assert_eq!(e.boundary_proc(g), proc);
    assert_ne!(e.boundary_proc(other), proc);
    assert_eq!(e.proc_owner(proc), Some(g));
    e.destroy_now(g);
    assert_eq!(e.proc_owner(proc), None);
}

// --------------------------------------------------------------- scripts

const SCRIPT: &str = r#"import QtQuick
Item {
    id: root
    property int hits: 0
    property var bomb: null
    signal ping()
    onPing: { throw new TypeError("first") }
    function inner() { throw new TypeError("deep") }
    function outer() { inner() }
    function careful() { try { outer() } catch (e) { return "caught " + e.message } }
    function go() { bomb.explode(); return 1 }
    function count() { hits += 1; return hits }
}
"#;

#[test]
fn an_exception_in_a_handler_does_not_stop_the_next_handler() {
    let h = engine();
    let e = &h.engine;
    let root = h.load(SCRIPT);
    let ping = e.signal_index(root, "ping").unwrap();
    let ran = Rc::new(Cell::new(0));
    let r = ran.clone();
    e.connect(root, ping, move |_, _| r.set(r.get() + 1))
        .unwrap();
    e.emit(root, ping, &[]).unwrap();
    assert_eq!(ran.get(), 1);
    let faults = e.faults();
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0].kind(), "JsException");
    assert_eq!(faults[0].message(), "TypeError: first");
    assert_eq!(faults[0].line, 7);
    assert_eq!(faults[0].origin, Some(root));
    assert!(faults[0].file.as_deref().unwrap().ends_with("Main.qml"));
    assert_eq!(h.warnings().len(), 1, "logged once");
    // With no boundary that asks for exceptions, logging is all.
    assert!(!e.has_pending_faults(), "nobody is woken for it");
    assert_eq!(e.deliver_faults(), 0);
    assert!(e.application_fault().is_none());
    assert_eq!(
        e.invoke_by_name(root, "count", &[]).unwrap(),
        Value::from(1.0)
    );
}

#[test]
fn an_exception_carries_the_qml_functions_it_passed_through() {
    let h = engine();
    let e = &h.engine;
    let root = h.load(SCRIPT);
    let _ = e.invoke_by_name(root, "outer", &[]);
    let faults = e.faults();
    assert_eq!(faults.len(), 1);
    let f = &faults[0];
    assert_eq!(f.message(), "TypeError: deep");
    assert_eq!(f.line, 8);
    let names: Vec<_> = f
        .stack
        .iter()
        .map(|s| (s.function.as_deref(), s.line))
        .collect();
    assert_eq!(names, [(Some("inner"), 8), (Some("outer"), 9)]);
    assert!(f.stack.iter().all(|s| s.file.ends_with("Main.qml")));
    let warnings = h.warnings();
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0].contains("Main.qml:8: TypeError: deep"),
        "{warnings:?}"
    );
    assert!(
        warnings[0].contains("at outer (Main.qml:9)"),
        "{warnings:?}"
    );
}

#[test]
fn an_exception_a_script_catches_is_not_a_fault() {
    let h = engine();
    let e = &h.engine;
    let root = h.load(SCRIPT);
    let r = e.invoke_by_name(root, "careful", &[]).unwrap();
    assert_eq!(r, Value::from("caught deep"));
    assert!(e.faults().is_empty());
    assert!(h.warnings().is_empty());
}

#[test]
fn a_panic_in_a_native_method_a_script_calls_is_a_fault() {
    let h = engine();
    let e = &h.engine;
    let root = h.load(SCRIPT);
    let b = bomb(e, Some(root));
    h.set(root, "bomb", Value::Object(b));
    let _ = e.invoke_by_name(root, "go", &[]);
    let faults = e.faults();
    let panic = faults
        .iter()
        .find(|f| f.kind() == "Panic")
        .expect("the panic is a fault");
    assert_eq!(panic.message(), "boom");
    assert_eq!(panic.origin, Some(b));
    e.deliver_faults();
    assert_eq!(e.application_fault().unwrap().kind(), "Panic");
    // The JS engine is as usable as the rest.
    h.log();
    assert_eq!(
        e.invoke_by_name(root, "count", &[]).unwrap(),
        Value::from(1.0)
    );
    assert!(h.warnings().is_empty());
}

#[test]
fn a_boundary_takes_exceptions_only_when_it_asks() {
    for (catch, expected) in [
        (false, vec![]),
        (true, vec!["stop g JsException: TypeError: deep", "start g"]),
    ] {
        let (h, events) = guarded();
        let e = &h.engine;
        let g = guard(e, None, "g", |g| g.catch_exceptions = catch);
        h.files(&[("Main", SCRIPT)]);
        let root = e.create_from_url("Main", Some(g), Vec::new()).unwrap();
        assert_eq!(e.boundary_of(root), Some(g));
        let _ = e.invoke_by_name(root, "outer", &[]);
        e.deliver_faults();
        assert_eq!(taken(&events), expected, "catch_exceptions: {catch}");
        assert_eq!(e.is_alive(root), !catch);
        assert!(e.application_fault().is_none());
    }
}

#[test]
fn an_exception_skips_a_boundary_that_does_not_ask_for_one_that_does() {
    let (h, events) = guarded();
    let e = &h.engine;
    let outer = guard(e, None, "outer", |g| g.catch_exceptions = true);
    let inner = guard(e, Some(outer), "inner", |_| {});
    h.files(&[("Main", SCRIPT)]);
    let root = e.create_from_url("Main", Some(inner), Vec::new()).unwrap();
    let _ = e.invoke_by_name(root, "outer", &[]);
    e.deliver_faults();
    assert_eq!(
        taken(&events),
        ["stop outer JsException: TypeError: deep", "start outer"]
    );
    assert!(!e.is_alive(inner));
}

// ------------------------------------------------------------ incubation

const INCUBATE: &str = r#"import QtQuick
Item {
    id: root
    property var inc: null
    property string seen: ""
    Component {
        id: comp
        Item {
            objectName: "kid"
            property int n: 1
            Item {} Item {} Item {} Item {} Item {} Item {} Item {} Item {}
        }
    }
    function start() {
        inc = comp.incubateObject(root, { n: 5 })
        inc.onStatusChanged = function(s) { seen += s + " " }
        return inc.status
    }
    function force() { inc.forceCompletion(); return inc.status }
}
"#;

/// What the incubator's `onStatusChanged` was called with.
fn statuses(h: &Harness, root: ObjectId) -> String {
    h.get(root, "seen").to_js_string().to_string()
}

#[test]
fn an_incubation_is_made_in_steps_and_leaves_nothing_scheduled() {
    let h = engine();
    let e = &h.engine;
    let root = h.load(INCUBATE);
    assert_eq!(
        e.invoke_by_name(root, "start", &[]).unwrap(),
        Value::from(2.0)
    );
    assert!(e.has_incubations());
    assert!(e.has_half_made() || support::find(e, root, "kid").is_none());
    let mut jobs = e.take_incubations();
    assert_eq!(jobs.len(), 1);
    let mut steps = 0;
    // One step at a time: the job is asked to stop after each.
    while matches!(jobs[0].step(&|| true), JobPoll::Yield) {
        steps += 1;
        assert!(steps < 1000, "the job never ends");
        let inc = h.get(root, "inc").as_object().unwrap();
        assert_eq!(h.num(inc, "status"), 2.0, "loading until it is done");
    }
    assert!(steps > 8, "a step per object at least, took {steps}");
    let inc = h.get(root, "inc").as_object().unwrap();
    assert_eq!(h.num(inc, "status"), 1.0);
    let kid = h
        .get(inc, "object")
        .as_object()
        .expect("the object is made");
    assert_eq!(h.num(kid, "n"), 5.0);
    assert_eq!(statuses(&h, root), "1 ");
    assert!(!e.has_incubations(), "nothing is queued once it is done");
    assert!(!e.has_half_made());
    assert!(e.next_timer_deadline().is_none());
    assert!(h.warnings().is_empty());
}

#[test]
fn force_completion_finishes_before_it_returns_and_the_job_has_nothing_left() {
    let h = engine();
    let e = &h.engine;
    let root = h.load(INCUBATE);
    e.invoke_by_name(root, "start", &[]).unwrap();
    let mut jobs = e.take_incubations();
    assert!(matches!(jobs[0].step(&|| true), JobPoll::Yield));
    assert_eq!(
        e.invoke_by_name(root, "force", &[]).unwrap(),
        Value::from(1.0)
    );
    assert_eq!(statuses(&h, root), "1 ");
    assert!(!e.has_half_made());
    // The queued half of the job finds the work done.
    assert!(matches!(jobs[0].step(&|| true), JobPoll::Done));
    assert_eq!(statuses(&h, root), "1 ", "told once");
    assert_eq!(
        e.invoke_by_name(root, "force", &[]).unwrap(),
        Value::from(1.0)
    );
    assert!(h.warnings().is_empty());
}

/// Bindings that say when they run, `Component.onCompleted` on three
/// levels and a binding on a bindable property of Qt's (`objectName`).
const ORDERED: &str = r#"import QtQuick
Item {
    id: root
    property var inc: null
    property var holder: Item { objectName: "holder" }
    function note(what, v) { console.log(what); return v }
    Component {
        id: comp
        Item {
            id: top
            objectName: root.note("name top", "kid")
            width: root.note("width top", a.width + 1)
            Component.onCompleted: console.log("completed top")
            Item {
                id: a
                width: root.note("width a", b.width + 1)
                Component.onCompleted: console.log("completed a")
                Item {
                    id: b
                    width: root.note("width b", 1)
                    height: root.note("height b", top.height)
                    Component.onCompleted: console.log("completed b")
                }
            }
            Item {
                width: root.note("width c", 7)
                Component.onCompleted: console.log("completed c")
            }
        }
    }
    function atOnce() { return comp.createObject(holder) }
    function start() { inc = comp.incubateObject(holder) }
}
"#;

/// The eager mode, in which a creation runs its bindings as Qt does.
fn ordered() -> (Harness, ObjectId) {
    let h = engine();
    h.engine.set_eager_bindings(true);
    let root = h.load(ORDERED);
    h.log();
    (h, root)
}

#[test]
fn a_creation_made_in_steps_does_what_one_made_at_once_does_in_the_same_order() {
    let (h, root) = ordered();
    h.engine.invoke_by_name(root, "atOnce", &[]).unwrap();
    let at_once = h.log();
    // Six bindings, some of them a second time once what they read has a
    // value, and four `onCompleted`.
    assert!(at_once.len() >= 10, "{at_once:?}");
    assert_eq!(
        at_once
            .iter()
            .filter(|l| l.starts_with("completed"))
            .count(),
        4
    );

    for stop in [true, false] {
        let (h, root) = ordered();
        let e = &h.engine;
        e.invoke_by_name(root, "start", &[]).unwrap();
        let mut jobs = e.take_incubations();
        let mut steps = 1;
        while matches!(jobs[0].step(&|| stop), JobPoll::Yield) {
            steps += 1;
            assert!(steps < 1000, "the job never ends");
        }
        assert_eq!(h.log(), at_once, "asked to stop after every step: {stop}");
        if stop {
            // Loading, allocating, four objects applied, six bindings
            // run, four objects completed, four `onCompleted`.
            assert!(steps >= 2 + 4 + 6 + 4 + 4, "{steps} steps");
        } else {
            assert_eq!(steps, 1);
        }
        let kid = support::find(e, root, "kid").expect("made");
        assert_eq!(h.num(kid, "width"), 3.0);
    }
}

#[test]
fn a_binding_a_handler_and_a_hook_are_a_step_each() {
    let (h, root) = ordered();
    let e = &h.engine;
    e.invoke_by_name(root, "start", &[]).unwrap();
    let mut jobs = e.take_incubations();
    // What each step logs: never more than one binding's worth (a binding
    // runs the ones it reads first) or one object's `onCompleted`.
    let mut most = 0;
    let mut said = 0;
    while matches!(jobs[0].step(&|| true), JobPoll::Yield) {
        let log = h.log();
        said += log.len();
        most = most.max(log.len());
        assert!(
            log.iter().all(|l| l.starts_with("completed"))
                || log.iter().all(|l| !l.starts_with("completed")),
            "bindings and completion in one step: {log:?}"
        );
        if log.iter().any(|l| l.starts_with("completed")) {
            assert_eq!(log.len(), 1, "{log:?}");
        }
    }
    said += h.log().len();
    assert!(said >= 10, "{said}");
    // `width top` reads `a.width`, which reads `b.width`, and what read a
    // property before its binding ran runs again with it.
    assert!(most <= 4, "{most} in one step");
}

#[test]
fn a_creation_called_off_while_it_completes_leaves_nothing() {
    let (h, root) = ordered();
    let e = &h.engine;
    let holder = h.find(root, "holder");
    e.invoke_by_name(root, "start", &[]).unwrap();
    let mut jobs = e.take_incubations();
    // Until the first `onCompleted` has run: the bindings are through.
    let mut steps = 0;
    loop {
        assert!(matches!(jobs[0].step(&|| true), JobPoll::Yield));
        steps += 1;
        assert!(steps < 1000);
        if h.log().iter().any(|l| l.starts_with("completed")) {
            break;
        }
    }
    let kid = support::find(e, root, "kid").expect("allocated and named");
    e.destroy_now(holder);
    assert!(matches!(jobs[0].step(&|| true), JobPoll::Done));
    assert!(!e.is_alive(kid));
    assert!(!e.has_half_made());
    assert!(h.log().is_empty(), "no other handler ran");
}
