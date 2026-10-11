//! `ErrorBoundary` and Loader as UI boundaries: what a fault below them
//! stops, what takes its place, and when the fault goes further.

mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use ljos_engine::{Engine, Migrate, ObjectId, QmlObject, QtObject, Value};
use ljos_runtime::{Time, VirtualClock};

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
    assert_eq!(num(&e, boundary, "width"), 0.0, "the text has no size here");
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
