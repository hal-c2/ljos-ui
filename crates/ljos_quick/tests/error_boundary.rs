//! `ErrorBoundary` and Loader as UI boundaries: what a fault below them
//! stops, what takes its place, and when the fault goes further.

mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use ljos_engine::{Engine, Migrate, ObjectId, QmlObject, QtObject, Value};
use ljos_runtime::{Clock, Time, VirtualClock};

fn text(e: &Engine, o: ObjectId, name: &str) -> String {
    get(e, o, name).to_js_string().to_string()
}

const SIMPLE: &str = r#"
import QtQuick
import Ljos
Item {
    objectName: "root"
    Item { objectName: "neighbour" }
    ErrorBoundary {
        id: boundary
        objectName: "boundary"
        fallback: Text { objectName: "sorry"; text: boundary.fault.kind + ": " + boundary.fault.message }
        Rectangle {
            objectName: "content"; width: 100; height: 50
            property int n: 0
            Item { objectName: "deep" }
        }
    }
}
"#;

#[test]
fn a_boundary_makes_its_content() {
    let e = engine();
    let root = load(&e, SIMPLE);
    let boundary = find(&e, root, "boundary");
    assert_eq!(e.take_warnings(), Vec::<String>::new());
    assert_eq!(text(&e, boundary, "fault.kind"), "");
    let content = find(&e, root, "content");
    assert_eq!(get(&e, boundary, "item").as_object(), Some(content));
    assert_eq!(num(&e, boundary, "width"), 100.0);
    assert_eq!(num(&e, boundary, "height"), 50.0);
}

/// A panic in a native handler of `obj`, as a host's turn catches it.
fn panic_in(e: &Engine, obj: ObjectId, message: &'static str) {
    let r: Option<()> = e.turn(Some(obj), || panic!("{message}"));
    assert!(r.is_none());
}

fn exists(e: &Engine, root: ObjectId, name: &str) -> bool {
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        if !e.is_alive(o) {
            continue;
        }
        if text(e, o, "objectName") == name {
            return true;
        }
        stack.extend(e.children(o));
    }
    false
}

#[test]
fn a_panic_below_the_boundary_shows_the_fallback_in_its_place() {
    let e = engine();
    let root = load(&e, SIMPLE);
    let boundary = find(&e, root, "boundary");
    let content = find(&e, root, "content");
    let mut s = scene(&e, root, 300.0, 200.0);
    frame(&mut s, &e);
    let boundary_node = s.node_of(boundary).unwrap();
    let content_node = s.node_of(content).unwrap();
    assert_eq!(
        s.render_tree().node(boundary_node).unwrap().children(),
        &[content_node]
    );
    let woke = Rc::new(Cell::new(0));
    let w = Rc::clone(&woke);
    s.set_waker(move || w.set(w.get() + 1));
    assert!(!s.needs_frame());

    panic_in(&e, find(&e, root, "deep"), "boom");
    // Nothing is destroyed where the panic is caught; the host is asked
    // for a frame to handle it in.
    assert!(e.is_alive(content));
    assert_eq!(text(&e, boundary, "failed"), "false");
    assert!(woke.get() >= 1);

    let out = frame(&mut s, &e);
    assert!(out.changed);
    assert!(!e.is_alive(content));
    assert!(!exists(&e, root, "deep"));
    assert!(e.is_alive(find(&e, root, "neighbour")));
    let sorry = find(&e, root, "sorry");
    assert_eq!(text(&e, sorry, "text"), "Panic: boom");
    assert_eq!(text(&e, boundary, "failed"), "true");
    assert_eq!(text(&e, boundary, "fault.kind"), "Panic");
    assert_eq!(text(&e, boundary, "fault.message"), "boom");
    assert_eq!(num(&e, boundary, "fault.line"), 14.0);
    assert!(text(&e, boundary, "fault.file").ends_with("Main.qml"));
    assert_eq!(get(&e, boundary, "item").as_object(), Some(sorry));
    assert_eq!(get(&e, sorry, "parent").as_object(), Some(boundary));
    let sorry_node = s.node_of(sorry).expect("the fallback is in the scene");
    assert_eq!(
        s.render_tree().node(boundary_node).unwrap().children(),
        &[sorry_node]
    );
    assert!(e.application_fault().is_none(), "the boundary took it");
    // And then nothing is scheduled.
    let out = frame(&mut s, &e);
    assert_eq!(out.next_wake, None);
    assert!(!s.needs_frame());
    assert!(!e.has_pending_faults());
}

#[test]
fn retry_makes_the_content_again_and_takes_the_fallback_away() {
    let e = engine();
    let root = load(&e, SIMPLE);
    let boundary = find(&e, root, "boundary");
    let first = find(&e, root, "content");
    set(&e, first, "n", Value::from(7.0));
    let mut s = scene(&e, root, 300.0, 200.0);
    frame(&mut s, &e);
    // Nothing to retry while the content is up.
    e.invoke_by_name(boundary, "retry", &[]).unwrap();
    assert_eq!(find(&e, root, "content"), first);
    assert_eq!(num(&e, boundary, "restarts"), 0.0);

    panic_in(&e, first, "boom");
    frame(&mut s, &e);
    assert_eq!(
        num(&e, boundary, "restarts"),
        0.0,
        "a fallback is no restart"
    );
    assert!(
        num(&e, boundary, "width") > 0.0,
        "the fallback is laid out in the frame that made it"
    );
    e.invoke_by_name(boundary, "retry", &[]).unwrap();
    let second = find(&e, root, "content");
    assert_ne!(second, first);
    assert_eq!(num(&e, second, "n"), 0.0, "made from the component");
    assert_eq!(text(&e, boundary, "failed"), "false");
    assert_eq!(text(&e, boundary, "fault.kind"), "");
    assert_eq!(text(&e, boundary, "fault.message"), "");
    assert_eq!(num(&e, boundary, "fault.line"), 0.0);
    assert_eq!(num(&e, boundary, "restarts"), 1.0);
    assert_eq!(get(&e, boundary, "item").as_object(), Some(second));
    assert_eq!(num(&e, boundary, "width"), 100.0);
    frame(&mut s, &e);
    assert!(!exists(&e, root, "sorry"), "destroyed at the next frame");
    assert!(s.node_of(second).is_some());
    let out = frame(&mut s, &e);
    assert_eq!(out.next_wake, None);
    assert!(e.take_warnings().len() == 1, "only the panic was logged");
}

#[test]
fn a_fault_in_the_fallback_is_not_the_boundarys_own() {
    let e = engine();
    let root = load(&e, SIMPLE);
    let boundary = find(&e, root, "boundary");
    let mut s = scene(&e, root, 300.0, 200.0);
    panic_in(&e, find(&e, root, "content"), "boom");
    frame(&mut s, &e);
    let sorry = find(&e, root, "sorry");
    assert_eq!(e.boundary_of(sorry), None);
    panic_in(&e, sorry, "the fallback too");
    frame(&mut s, &e);
    // The boundary cannot contain what it shows for a fault: it goes on.
    let fault = e.application_fault().expect("nothing is around it");
    assert_eq!(fault.message(), "the fallback too");
    assert!(e.is_alive(sorry));
    assert_eq!(text(&e, boundary, "fault.message"), "boom");
}

/// What a frame does about faults, for a test without a scene: hands
/// them to their boundaries and destroys what those discarded.
fn deliver(e: &Engine) {
    e.deliver_faults();
    e.process_deferred_deletes();
}

const NESTED: &str = r#"
import QtQuick
import Ljos
Item {
    objectName: "root"
    ErrorBoundary {
        id: outer
        objectName: "outer"
        maxRestarts: 5
        Item {
            objectName: "page"
            ErrorBoundary {
                id: inner
                objectName: "inner"
                maxRestarts: 2
                within: 1000
                Item { objectName: "leaf" }
            }
        }
    }
}
"#;

/// An engine on a clock the test moves, as a boundary counts its restarts
/// on it.
fn timed() -> (Engine, Arc<VirtualClock>) {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    (e, clock)
}

#[test]
fn a_boundary_without_a_fallback_makes_its_content_again_at_once() {
    let (e, _clock) = timed();
    let root = load(&e, NESTED);
    let inner = find(&e, root, "inner");
    let first = find(&e, root, "leaf");
    assert_eq!(e.boundary_of(first), Some(inner));
    assert_eq!(e.boundary_of(inner), Some(find(&e, root, "outer")));
    panic_in(&e, first, "one");
    deliver(&e);
    let second = find(&e, root, "leaf");
    assert_ne!(second, first);
    assert_eq!(find(&e, root, "inner"), inner, "only the content went");
    assert_eq!(num(&e, inner, "restarts"), 1.0);
    assert_eq!(text(&e, inner, "failed"), "false");
    // The fault that caused the restart stays readable.
    assert_eq!(text(&e, inner, "fault.message"), "one");
    assert_eq!(e.boundary_recent_restarts(inner), 1);
}

#[test]
fn a_boundary_over_its_intensity_hands_the_fault_to_the_one_around_it() {
    let (e, clock) = timed();
    let root = load(&e, NESTED);
    let outer = find(&e, root, "outer");
    let inner = find(&e, root, "inner");
    let page = find(&e, root, "page");
    for message in ["one", "two"] {
        panic_in(&e, find(&e, root, "leaf"), message);
        deliver(&e);
        clock.advance(Duration::from_millis(100));
    }
    assert_eq!(num(&e, inner, "restarts"), 2.0);
    assert_eq!(num(&e, outer, "restarts"), 0.0);
    panic_in(&e, find(&e, root, "leaf"), "three");
    deliver(&e);
    // The outer content was made again, the inner boundary with it.
    assert_eq!(num(&e, outer, "restarts"), 1.0);
    assert_eq!(text(&e, outer, "fault.message"), "three");
    assert!(!e.is_alive(page));
    assert!(!e.is_alive(inner));
    let inner = find(&e, root, "inner");
    assert_eq!(num(&e, inner, "restarts"), 0.0);
    assert!(e.application_fault().is_none());
    let dump = e.supervisor_dump().unwrap();
    assert!(dump.contains("boundary"), "{dump}");
}

#[test]
fn restarts_that_are_far_apart_never_escalate() {
    let (e, clock) = timed();
    let root = load(&e, NESTED);
    let outer = find(&e, root, "outer");
    let inner = find(&e, root, "inner");
    for i in 0..6 {
        panic_in(&e, find(&e, root, "leaf"), "again");
        deliver(&e);
        assert_eq!(num(&e, inner, "restarts"), f64::from(i) + 1.0);
        clock.advance(Duration::from_millis(600));
    }
    assert_eq!(num(&e, outer, "restarts"), 0.0);
    assert_eq!(find(&e, root, "inner"), inner);
}

#[test]
fn the_outermost_boundary_over_its_intensity_tells_the_application() {
    let (e, _clock) = timed();
    let root = load(&e, &NESTED.replace("maxRestarts: 5", "maxRestarts: 0"));
    let outer = find(&e, root, "outer");
    let told = Rc::new(Cell::new(0));
    let t = Rc::clone(&told);
    e.set_fault_handler(move |_, _| t.set(t.get() + 1));
    for message in ["one", "two", "three"] {
        panic_in(&e, find(&e, root, "leaf"), message);
        deliver(&e);
    }
    assert_eq!(told.get(), 1);
    let fault = e
        .application_fault()
        .expect("no boundary is left to take it");
    assert_eq!(fault.kind(), "Panic");
    assert_eq!(fault.message(), "three");
    assert_eq!(text(&e, outer, "failed"), "true");
    assert!(get(&e, outer, "item").is_nullish());
    assert!(!exists(&e, root, "page"));
    // The application can still try again.
    e.invoke_by_name(outer, "retry", &[]).unwrap();
    assert!(exists(&e, root, "leaf"));
    assert_eq!(text(&e, outer, "failed"), "false");
}

/// A native type with state its component does not declare. Under the name
/// of a type that is not `ljos_quick`'s to implement (the controls have
/// it), so the compiler knows it whichever table of types the build reads.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "ButtonGroup", extends = QtObject)]
struct Tally {
    #[qml(base)]
    base: QtObject,
    #[qml(skip)]
    marks: Vec<u32>,
}

impl Migrate for Tally {
    const VERSION: u32 = 1;
    type State = Vec<u32>;

    fn snapshot(e: &Engine, obj: ObjectId) -> Option<Vec<u32>> {
        e.with::<Tally, _>(obj, |t| t.marks.clone())
            .ok()
            .filter(|m| !m.is_empty())
    }

    fn migrate(e: &Engine, obj: ObjectId, _from: u32, marks: Vec<u32>) -> Result<(), String> {
        e.with_mut::<Tally, _>(obj, |t| t.marks = marks)
            .map_err(|err| err.to_string())
    }
}

const KEEPING: &str = r#"
import QtQuick
import QtQuick.Templates
import Ljos
Item {
    objectName: "root"
    ErrorBoundary {
        objectName: "boundary"
        FALLBACK
        Item {
            objectName: "content"
            property int n: 0
            ButtonGroup { objectName: "tally" }
        }
    }
}
"#;

fn keeping(fallback: &str) -> (Engine, ObjectId) {
    let e = engine();
    e.register_module(ljos_engine::module!("QtQuick.Templates" 2.0 => [Tally]));
    e.register_migrate::<Tally>();
    let root = load(&e, &KEEPING.replace("FALLBACK", fallback));
    (e, root)
}

fn marks(e: &Engine, root: ObjectId) -> Vec<u32> {
    e.with::<Tally, _>(find(e, root, "tally"), |t| t.marks.clone())
        .unwrap()
}

#[test]
fn a_restart_keeps_what_a_native_type_says_is_worth_keeping() {
    let (e, root) = keeping("");
    let first = find(&e, root, "tally");
    e.with_mut::<Tally, _>(first, |t| t.marks = vec![3, 1, 4])
        .unwrap();
    set(&e, find(&e, root, "content"), "n", Value::from(9.0));
    panic_in(&e, first, "boom");
    deliver(&e);
    let second = find(&e, root, "tally");
    assert_ne!(second, first);
    assert_eq!(marks(&e, root), [3, 1, 4]);
    // What the component declares is made from the component.
    assert_eq!(num(&e, find(&e, root, "content"), "n"), 0.0);
    assert_eq!(e.take_warnings().len(), 1, "only the panic was logged");
}

#[test]
fn what_was_kept_waits_for_retry_behind_a_fallback() {
    let (e, root) = keeping("fallback: Item { objectName: \"sorry\" }");
    let boundary = find(&e, root, "boundary");
    let first = find(&e, root, "tally");
    e.with_mut::<Tally, _>(first, |t| t.marks = vec![2, 7])
        .unwrap();
    panic_in(&e, first, "boom");
    deliver(&e);
    assert!(exists(&e, root, "sorry"));
    assert!(!exists(&e, root, "tally"));
    e.invoke_by_name(boundary, "retry", &[]).unwrap();
    assert_eq!(marks(&e, root), [2, 7]);
}

const LOADER: &str = r#"
import QtQuick
Item {
    objectName: "root"
    Component { id: page; Item { objectName: "page"; Item { objectName: "leaf" } } }
    Loader { id: loader; objectName: "loader"; sourceComponent: page }
}
"#;

#[test]
fn a_loader_is_a_boundary_that_stays_stopped() {
    let e = engine();
    let root = load(&e, LOADER);
    let loader = find(&e, root, "loader");
    let page = find(&e, root, "page");
    let leaf = find(&e, root, "leaf");
    assert_eq!(e.boundary_of(leaf), Some(loader));
    assert_eq!(e.proc_for(leaf), e.boundary_proc(loader));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    panic_in(&e, leaf, "boom");
    frame(&mut s, &e);
    assert!(!e.is_alive(page));
    assert!(get(&e, loader, "item").is_nullish());
    assert_eq!(num(&e, loader, "status"), 3.0);
    assert!(e.application_fault().is_none(), "the Loader took it");
    let out = frame(&mut s, &e);
    assert_eq!(out.next_wake, None);
    // Loading again is the application's to ask for.
    set(&e, loader, "active", Value::Bool(false));
    set(&e, loader, "active", Value::Bool(true));
    assert_eq!(num(&e, loader, "status"), 1.0);
    assert!(exists(&e, root, "leaf"));
    // And the new item is supervised like the first.
    panic_in(&e, find(&e, root, "leaf"), "again");
    frame(&mut s, &e);
    assert_eq!(num(&e, loader, "status"), 3.0);
    let out = frame(&mut s, &e);
    assert_eq!(out.next_wake, None::<Time>);
    assert!(!s.needs_frame());
}

#[test]
fn an_exception_does_not_unload_a_loader() {
    let e = engine();
    let root = load(
        &e,
        &LOADER.replace(
            "Item { objectName: \"leaf\" }",
            "function fail() { throw new TypeError(\"not this\") }",
        ),
    );
    let loader = find(&e, root, "loader");
    let page = find(&e, root, "page");
    let _ = e.invoke_by_name(page, "fail", &[]);
    deliver(&e);
    assert!(e.is_alive(page));
    assert_eq!(num(&e, loader, "status"), 1.0);
    assert!(e.application_fault().is_none());
    assert_eq!(e.take_warnings().len(), 1, "logged, as Qt does");
}

const RUNS_OUT: &str = r#"
import QtQuick
Item {
    id: root
    objectName: "root"
    property bool on: false
    Loader { objectName: "loader"; active: root.on; source: "Row.qml" }
    // Cycles only the collector frees, as a heap left full of garbage.
    function litter(n) {
        for (let i = 0; i < n; i++) { const a = { b: [i, i + 1, "x" + i] }; a.self = a }
    }
}
"#;

/// A file whose item needs its script while it is made, and the script
/// takes room to load: tens of kilobytes, against the few left free.
fn row() -> String {
    let mut s = String::from("import QtQuick\nItem {\n    width: f0(1, 2).length\n");
    for i in 0..40 {
        s.push_str(&format!(
            "    function f{i}(a, b) {{ const t = [a, b, \"row {i}\"]; return t.map(x => String(x) + {i}).join(\"-\") }}\n"
        ));
    }
    s.push_str("}\n");
    s
}

/// A Loader whose item's script the heap had no room for, `room` bytes
/// above what the heap holds after `litter` left `garbage` cycles in it.
fn out_of_room(garbage: u32, room: usize) -> (Engine, ObjectId, ObjectId) {
    let e = engine();
    let (root, loader) = out_of_room_on(&e, garbage, room);
    (e, root, loader)
}

/// [`out_of_room`] on an engine of the test's own (on a clock it moves).
fn out_of_room_on(e: &Engine, garbage: u32, room: usize) -> (ObjectId, ObjectId) {
    compile(e, &[("Main", RUNS_OUT), ("Row", &row())]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let loader = find(e, root, "loader");
    assert_eq!(num(e, loader, "status"), 0.0);
    // Running a script starts the script engine.
    e.invoke_by_name(root, "litter", &[Value::from(0.0)])
        .unwrap();
    assert!(e.collect_garbage());
    e.invoke_by_name(root, "litter", &[Value::from(f64::from(garbage))])
        .unwrap();
    let held = e.script_memory().unwrap().bytes;
    assert!(e.set_script_heap_limit(Some(held + room)));
    set(e, root, "on", Value::Bool(true));
    // The item is made, but its script did not load: what QuickJS could not
    // allocate is a fault for the Loader, which it hears at the next frame.
    assert_eq!(num(e, loader, "status"), 1.0);
    (root, loader)
}

#[test]
fn a_loader_whose_item_ran_out_of_heap_loads_again_after_a_collection() {
    let (e, root, loader) = out_of_room(20_000, 4 * 1024);
    let warned = e.take_warnings();
    assert!(
        warned.iter().any(|w| w.contains("JS heap limit exceeded")),
        "{warned:?}"
    );
    // At the next frame the faults go to their boundaries: the Loader
    // collects the garbage and has room to load again.
    let first = get(&e, loader, "item").as_object();
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    frame(&mut s, &e);
    assert_eq!(num(&e, loader, "status"), 1.0);
    let item = get(&e, loader, "item").as_object();
    assert!(item.is_some() && item != first);
    assert_eq!(num(&e, item.unwrap(), "width"), 12.0, "its script loaded");
    let warned = e.take_warnings();
    assert!(
        warned
            .iter()
            .any(|w| w.contains("Loader: its item ran out of script memory, loading again")),
        "{warned:?}"
    );
    assert!(e.application_fault().is_none());
}

#[test]
fn a_loader_whose_item_runs_out_of_heap_again_waits_and_loads_again() {
    // No garbage to collect: the heap stays full, and loading again runs
    // out again.
    let (e, clock) = timed();
    let (root, loader) = out_of_room_on(&e, 0, 4 * 1024);
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    frame(&mut s, &e);
    assert_eq!(num(&e, loader, "status"), 3.0);
    let warned = e.take_warnings();
    assert!(
        warned.iter().any(|w| w.contains(
            "Loader: its item ran out of script memory again, so it stays at Loader.Error and loads again in 1 s"
        )),
        "{warned:?}"
    );
    // It waits, without polling: one wake, when the wait is over.
    // A second from the fault, which came after the script ran a while.
    let due = e.next_timer_deadline().expect("it waits to load again");
    let wait = due.saturating_since(clock.now());
    assert!(
        wait >= Duration::from_secs(1) && wait < Duration::from_millis(1100),
        "{wait:?}"
    );
    clock.set(due.saturating_sub(Duration::from_millis(500)));
    e.advance_timers(clock.now());
    assert_eq!(num(&e, loader, "status"), 3.0);
    // Loading again runs out again: twice as long a wait.
    clock.set(due);
    e.advance_timers(clock.now());
    frame(&mut s, &e);
    assert_eq!(num(&e, loader, "status"), 3.0);
    let warned = e.take_warnings();
    assert!(
        warned.iter().any(|w| w.contains("loads again in 2 s")),
        "{warned:?}"
    );
    // Whatever filled the heap lets go: the next load finds room, with
    // nobody telling the Loader to load.
    assert!(e.set_script_heap_limit(None));
    clock.advance(Duration::from_secs(2));
    e.advance_timers(clock.now());
    frame(&mut s, &e);
    assert_eq!(num(&e, loader, "status"), 1.0);
    let item = get(&e, loader, "item").as_object().unwrap();
    assert_eq!(num(&e, item, "width"), 12.0, "its script loaded");
    assert!(e.application_fault().is_none());
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
}

/// Rows whose scripts run out of a heap full of what lives do not each
/// collect it: a collection right after one finds nothing new to free.
#[test]
fn rows_that_run_out_of_a_full_heap_together_do_not_each_collect_it() {
    let (e, _clock) = timed();
    let rows = RUNS_OUT.replace(
        "Loader { objectName: \"loader\"; active: root.on; source: \"Row.qml\" }",
        "Loader { objectName: \"loader\"; active: root.on; source: \"Row.qml\" }\n\
         Loader { active: root.on; source: \"Row.qml\" }\n\
         Loader { active: root.on; source: \"Row.qml\" }\n\
         Loader { active: root.on; source: \"Row.qml\" }",
    );
    compile(&e, &[("Main", &rows), ("Row", &row())]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    e.invoke_by_name(root, "litter", &[Value::from(0.0)])
        .unwrap();
    assert!(e.collect_garbage());
    let held = e.script_memory().unwrap().bytes;
    assert!(e.set_script_heap_limit(Some(held + 4 * 1024)));
    let before = e.script_memory().unwrap().collections;
    set(&e, root, "on", Value::Bool(true));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    frame(&mut s, &e);
    let loader = find(&e, root, "loader");
    assert_eq!(num(&e, loader, "status"), 3.0);
    let collected = e.script_memory().unwrap().collections - before;
    assert!(collected <= 1, "{collected} collections");
}

#[test]
fn a_loader_counts_a_late_fault_from_when_it_loaded_again() {
    // The fault is heard long after it came: the load it sets off is the
    // one the next fault comes soon after, not the fault.
    let (e, clock) = timed();
    let (root, loader) = out_of_room_on(&e, 0, 4 * 1024);
    clock.advance(Duration::from_secs(30));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    frame(&mut s, &e);
    assert_eq!(num(&e, loader, "status"), 3.0);
    let warned = e.take_warnings();
    let at_once = warned
        .iter()
        .filter(|w| w.ends_with("Loader: its item ran out of script memory, loading again"))
        .count();
    assert_eq!(at_once, 1, "loaded at once only for the first: {warned:?}");
    assert!(
        warned.iter().any(|w| w.contains(
            "Loader: its item ran out of script memory again, so it stays at Loader.Error and loads again in 1 s"
        )),
        "{warned:?}"
    );
    assert!(e.next_timer_deadline().is_some(), "it waits to load again");
}

#[test]
fn a_loader_told_to_load_while_it_waits_loads_at_once_and_drops_the_wait() {
    let (e, clock) = timed();
    let (root, loader) = out_of_room_on(&e, 0, 4 * 1024);
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    frame(&mut s, &e);
    assert_eq!(num(&e, loader, "status"), 3.0);
    assert!(e.next_timer_deadline().is_some());
    // Told to load once there is room, it does.
    assert!(e.set_script_heap_limit(None));
    set(&e, root, "on", Value::Bool(false));
    set(&e, root, "on", Value::Bool(true));
    assert_eq!(num(&e, loader, "status"), 1.0);
    let item = get(&e, loader, "item").as_object();
    assert_eq!(e.next_timer_deadline(), None, "the wait went with the load");
    clock.advance(Duration::from_secs(5));
    e.advance_timers(clock.now());
    assert_eq!(
        get(&e, loader, "item").as_object(),
        item,
        "not loaded twice"
    );
}

#[test]
fn a_loader_whose_item_always_runs_out_never_gives_up_and_waits_at_most_a_minute() {
    let (e, clock) = timed();
    let (root, loader) = out_of_room_on(&e, 0, 4 * 1024);
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    frame(&mut s, &e);
    let mut waits = Vec::new();
    for _ in 0..9 {
        assert_eq!(num(&e, loader, "status"), 3.0);
        let due = e.next_timer_deadline().expect("a load is always to come");
        waits.push(due.saturating_since(clock.now()).as_secs());
        clock.advance(due.saturating_since(clock.now()));
        e.advance_timers(clock.now());
        frame(&mut s, &e);
    }
    assert_eq!(waits, [1, 2, 4, 8, 16, 32, 60, 60, 60]);
    assert!(e.application_fault().is_none());
}

const ROWS: &str = r#"
import QtQuick
Item {
    objectName: "root"
    Component { id: row; Item { objectName: "cell" } }
    Loader { objectName: "a"; sourceComponent: row }
    Loader { objectName: "b"; sourceComponent: row }
    Loader { objectName: "c"; sourceComponent: row }
    function warm() { return 1 }
    function litter(n) {
        for (let i = 0; i < n; i++) { const a = { b: [i, i + 1, "x" + i] }; a.self = a }
    }
}
"#;

#[test]
fn loaders_that_run_out_of_heap_together_collect_once() {
    let e = engine();
    let root = load(&e, ROWS);
    // Running a script starts the script engine.
    e.invoke_by_name(root, "warm", &[]).unwrap();
    let loaders = ["a", "b", "c"].map(|n| find(&e, root, n));
    let cells = loaders.map(|l| get(&e, l, "item").as_object().unwrap());
    for cell in cells {
        e.report_fault(Some(cell), ljos_runtime::Fault::JsOutOfMemory);
    }
    let before = e.script_memory().unwrap().collections;
    deliver(&e);
    assert_eq!(e.script_memory().unwrap().collections, before + 1);
    for (l, old) in loaders.into_iter().zip(cells) {
        assert_eq!(num(&e, l, "status"), 1.0);
        assert_ne!(get(&e, l, "item").as_object(), Some(old));
    }
    let warned = e.take_warnings();
    assert_eq!(
        warned
            .iter()
            .filter(|w| w.contains("Loader: its item ran out of script memory, loading again"))
            .count(),
        3,
        "{warned:?}"
    );
    // Running out again with nothing made since: collecting again would
    // free nothing new.
    let cell = get(&e, loaders[0], "item").as_object().unwrap();
    e.report_fault(Some(cell), ljos_runtime::Fault::JsOutOfMemory);
    deliver(&e);
    assert_eq!(e.script_memory().unwrap().collections, before + 1);
    // A heap that grew after that collection is collected for again.
    e.invoke_by_name(root, "litter", &[Value::from(20_000.0)])
        .unwrap();
    let cell = get(&e, loaders[1], "item").as_object().unwrap();
    e.report_fault(Some(cell), ljos_runtime::Fault::JsOutOfMemory);
    deliver(&e);
    assert_eq!(e.script_memory().unwrap().collections, before + 2);
}

#[test]
fn a_loader_out_of_heap_soon_after_an_interrupt_does_not_say_again() {
    let (e, _clock) = timed();
    let root = load(&e, LOADER);
    let loader = find(&e, root, "loader");
    e.report_fault(
        Some(find(&e, root, "leaf")),
        ljos_runtime::Fault::JsInterrupted {
            slice: Duration::from_millis(50),
        },
    );
    deliver(&e);
    assert_eq!(num(&e, loader, "status"), 1.0, "loaded again");
    e.report_fault(
        Some(find(&e, root, "leaf")),
        ljos_runtime::Fault::JsOutOfMemory,
    );
    deliver(&e);
    assert_eq!(num(&e, loader, "status"), 3.0);
    let warned = e.take_warnings();
    assert!(
        warned.iter().any(|w| w.contains(
            "Loader: its item ran out of script memory soon after a script was interrupted, so it stays at Loader.Error and loads again in 1 s"
        )),
        "{warned:?}"
    );
    assert!(
        !warned.iter().any(|w| w.contains("memory again")),
        "{warned:?}"
    );
}
