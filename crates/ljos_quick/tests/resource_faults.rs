//! Content a resource fault took is made again by whatever owns it
//! (ljos_engine::retry): a view's delegates, a singleton, a boundary, a
//! `Task`.

mod common;

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_runtime::{Clock, VirtualClock};

fn timed() -> (Engine, Arc<VirtualClock>) {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    (e, clock)
}

/// A file whose item needs its script while it is made, and the script
/// takes room to load.
fn row() -> String {
    let mut s = String::from(
        "import QtQuick\nItem {\n    objectName: \"row\"\n    width: f0(1, 2).length\n    height: 10\n",
    );
    for i in 0..40 {
        s.push_str(&format!(
            "    function f{i}(a, b) {{ const t = [a, b, \"row {i}\"]; return t.map(x => String(x) + {i}).join(\"-\") }}\n"
        ));
    }
    s.push_str("}\n");
    s
}

const MAIN: &str = r#"
import QtQuick
Item {
    id: root
    objectName: "root"
    property int rows: 0
    Column {
        objectName: "column"
        Repeater { objectName: "repeater"; model: root.rows; delegate: HeavyRow {} }
    }
    function litter(n) {
        for (let i = 0; i < n; i++) { const a = { b: [i, i + 1, "x" + i] }; a.self = a }
    }
}
"#;

/// `MAIN` with its scripts running and `room` bytes free in the heap.
fn full(e: &Engine, room: usize) -> ObjectId {
    compile(e, &[("Main", MAIN), ("HeavyRow", &row())]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    e.invoke_by_name(root, "litter", &[Value::from(0.0)])
        .unwrap();
    assert!(e.collect_garbage());
    let held = e.script_memory().unwrap().bytes;
    assert!(e.set_script_heap_limit(Some(held + room)));
    root
}

fn item_at(e: &Engine, repeater: ObjectId, i: usize) -> Option<ObjectId> {
    e.invoke_by_name(repeater, "itemAt", &[Value::from(i as f64)])
        .ok()
        .and_then(|v| v.as_object())
}

/// Moves the clock on by `by` and runs what is due, with a frame after.
fn later(e: &Engine, clock: &VirtualClock, s: &mut ljos_quick::QuickScene, by: Duration) {
    clock.advance(by);
    e.advance_timers(clock.now());
    frame(s, e);
    frame(s, e);
}

#[test]
fn a_repeater_row_that_ran_out_of_heap_is_made_again_once_there_is_room() {
    let (e, clock) = timed();
    let root = full(&e, 4 * 1024);
    let rep = find(&e, root, "repeater");
    set(&e, root, "rows", Value::from(3.0));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    frame(&mut s, &e);
    let warned = e.take_warnings();
    assert!(
        warned.iter().any(|w| w.contains("is made again")),
        "{warned:?} {:#?}",
        faults(&e)
    );
    assert!(
        e.application_fault().is_none(),
        "{:?}",
        e.application_fault()
    );
    // Room comes, without anyone telling the view.
    assert!(e.set_script_heap_limit(None));
    later(&e, &clock, &mut s, Duration::from_secs(61));
    for i in 0..3 {
        let row = item_at(&e, rep, i).unwrap_or_else(|| panic!("row {i} is there"));
        assert_eq!(num(&e, row, "width"), 12.0, "the script of row {i} loaded");
    }
    assert_eq!(num(&e, rep, "count"), 3.0);
    assert!(e.application_fault().is_none());
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
}

const ROWS: &str = r#"
import QtQuick
Item {
    id: root
    objectName: "root"
    property int made: 0
    Column {
        Repeater {
            objectName: "repeater"
            model: 2
            delegate: Item { width: 10; height: 10; Component.onCompleted: root.made += 1 }
        }
    }
    ListView {
        objectName: "view"
        y: 30; width: 100; height: 100
        model: 2
        delegate: Item { width: 100; height: 10; Component.onCompleted: root.made += 1 }
    }
}
"#;

#[test]
fn a_full_heap_in_a_repeater_row_makes_that_row_again() {
    let (e, clock) = timed();
    let root = load(&e, ROWS);
    let rep = find(&e, root, "repeater");
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    let made = num(&e, root, "made");
    let first = item_at(&e, rep, 0).unwrap();
    let other = item_at(&e, rep, 1).unwrap();
    e.report_fault(Some(first), ljos_runtime::Fault::JsOutOfMemory);
    frame(&mut s, &e);
    frame(&mut s, &e);
    assert!(!e.is_alive(first), "the row the fault took is gone");
    let again = item_at(&e, rep, 0).expect("made again");
    assert_ne!(again, first);
    assert_eq!(item_at(&e, rep, 1), Some(other), "the other row stays");
    assert_eq!(num(&e, root, "made"), made + 1.0);
    let warned = e.take_warnings();
    assert!(
        warned.iter().any(|w| w
            .contains("the script heap was full, so the delegate of row 0 is made again at once")),
        "{warned:?}"
    );
    assert!(e.application_fault().is_none());
    // Soon after, the next fault waits.
    e.report_fault(Some(again), ljos_runtime::Fault::JsOutOfMemory);
    frame(&mut s, &e);
    assert!(item_at(&e, rep, 0).is_none(), "it waits");
    let warned = e.take_warnings();
    assert!(
        warned.iter().any(|w| w.contains("made again in 1 s")),
        "{warned:?}"
    );
    later(&e, &clock, &mut s, Duration::from_secs(1));
    assert!(item_at(&e, rep, 0).is_some());
    assert!(e.application_fault().is_none());
}

#[test]
fn a_full_heap_in_a_list_view_row_makes_that_row_again() {
    let (e, _clock) = timed();
    let root = load(&e, ROWS);
    let view = find(&e, root, "view");
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    let first = ljos_quick::view_item_at(&e, view, 0).expect("row 0 is shown");
    let made = num(&e, root, "made");
    e.report_fault(
        Some(first),
        ljos_runtime::Fault::JsInterrupted {
            slice: Duration::from_millis(5),
        },
    );
    frame(&mut s, &e);
    frame(&mut s, &e);
    frame(&mut s, &e);
    assert!(!e.is_alive(first) || ljos_quick::view_item_at(&e, view, 0) != Some(first));
    let again = ljos_quick::view_item_at(&e, view, 0).expect("made again");
    assert_ne!(again, first);
    assert_eq!(num(&e, root, "made"), made + 1.0);
    let warned = e.take_warnings();
    assert!(
        warned.iter().any(|w| w
            .contains("a script was interrupted, so the delegate of row 0 is made again at once")),
        "{warned:?}"
    );
    assert!(e.application_fault().is_none());
}

#[test]
fn a_code_bug_in_a_row_is_not_made_again() {
    let (e, _clock) = timed();
    let root = load(&e, ROWS);
    let rep = find(&e, root, "repeater");
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    let first = item_at(&e, rep, 0).unwrap();
    let made = num(&e, root, "made");
    e.report_fault(Some(first), ljos_runtime::Fault::Panic("broken row".into()));
    frame(&mut s, &e);
    frame(&mut s, &e);
    assert_eq!(num(&e, root, "made"), made, "not made again");
    let fault = e.application_fault().expect("nothing is around it");
    assert_eq!(fault.message(), "broken row");
}

const BOUNDED: &str = r#"
import QtQuick
import Ljos
ErrorBoundary {
    objectName: "outer"
    fallback: Item { objectName: "outerSorry" }
    Item {
        objectName: "page"
        ErrorBoundary {
            objectName: "inner"
            maxRestarts: 1
            within: 600000
            fallback: Item { objectName: "sorry" }
            Item { objectName: "content" }
        }
    }
}
"#;

#[test]
fn an_error_boundary_never_gives_up_on_content_a_full_heap_took() {
    let (e, clock) = timed();
    let root = load(&e, BOUNDED);
    let inner = find(&e, root, "inner");
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    let mut waits = Vec::new();
    for n in 0..5 {
        let content = find(&e, root, "content");
        e.report_fault(Some(content), ljos_runtime::Fault::JsOutOfMemory);
        frame(&mut s, &e);
        assert!(e.is_alive(inner), "it never gives up");
        assert_eq!(get(&e, root, "failed"), Value::Bool(false), "nor escalates");
        let due = e.next_timer_deadline().expect("it waits to make it again");
        let wait = due.saturating_since(clock.now());
        waits.push(wait.as_secs());
        clock.set(due);
        e.advance_timers(clock.now());
        frame(&mut s, &e);
    }
    assert_eq!(waits, [0, 1, 2, 4, 8]);
    assert!(e.application_fault().is_none());
    // A code bug still counts: maxRestarts 1 within the window. It
    // shows the fallback until `retry()`.
    let content = find(&e, root, "content");
    e.report_fault(Some(content), ljos_runtime::Fault::Panic("one".into()));
    frame(&mut s, &e);
    assert_eq!(get(&e, inner, "failed"), Value::Bool(true));
    e.invoke_by_name(inner, "retry", &[]).unwrap();
    frame(&mut s, &e);
    let content = find(&e, root, "content");
    e.report_fault(Some(content), ljos_runtime::Fault::Panic("two".into()));
    frame(&mut s, &e);
    frame(&mut s, &e);
    assert_eq!(
        get(&e, root, "failed"),
        Value::Bool(true),
        "the outer took it"
    );
}

/// A JS clock on which every poll of the interrupt takes ten milliseconds
/// and native work none: a script that never returns is interrupted soon.
fn js_timed(e: &Engine) {
    use ljos_engine::JsTiming;
    use std::cell::Cell;
    let now = Rc::new(Cell::new(std::time::Instant::now()));
    e.set_js_clock(Some(Rc::new(move |t| {
        if let JsTiming::Poll = t {
            now.set(now.get() + Duration::from_millis(10));
        }
        now.get()
    })))
    .unwrap();
}

/// What faulted and what took each fault, for a failing assertion.
fn faults(e: &Engine) -> Vec<String> {
    e.faults()
        .iter()
        .map(|r| format!("{:?} in {:?}: {:?}", r.fault, r.origin_name, r.taken_by))
        .collect()
}

const INTERRUPTED_CONTENT: &str = r#"
import QtQuick
import Ljos
ErrorBoundary {
    objectName: "outer"
    fallback: Item { objectName: "outerSorry" }
    Item {
        id: root
        objectName: "root"
        property int tries: 0
        property bool always: false
        ErrorBoundary {
            objectName: "inner"
            maxRestarts: 1
            within: 60000
            fallback: Item { objectName: "sorry" }
            Item {
                objectName: "content"
                Component.onCompleted: {
                    root.tries += 1
                    if (root.always || root.tries === 1) for (;;) { }
                }
            }
        }
    }
}
"#;

#[test]
fn an_error_boundary_makes_content_whose_script_was_interrupted_again() {
    let (e, clock) = timed();
    js_timed(&e);
    let outer = load(&e, INTERRUPTED_CONTENT);
    let root = find(&e, outer, "root");
    let mut s = scene(&e, outer, 200.0, 200.0);
    later(&e, &clock, &mut s, Duration::ZERO);
    let warned = e.take_warnings();
    assert_eq!(num(&e, root, "tries"), 2.0, "{:?}", faults(&e));
    assert!(
        warned.iter().any(|w| w.contains(
            "ErrorBoundary: a script was interrupted, so its content is made again at once"
        )),
        "{warned:?}"
    );
    assert_eq!(get(&e, outer, "failed"), Value::Bool(false));
    assert!(
        e.application_fault().is_none(),
        "{:?}",
        e.application_fault()
    );
}

const INTERRUPTED_TASK: &str = r#"
import QtQuick
import Ljos.Core
import Ljos.Ui
Item {
    id: root; objectName: "root"
    property int tries: 0
    Task {
        id: work; objectName: "work"
        args: []
        run: async (signal: AbortSignal): Promise<string> => {
            root.tries += 1
            if (root.tries === 1) for (;;) { }
            return "made"
        }
    }
}
"#;

#[test]
fn a_task_whose_run_was_interrupted_as_it_started_starts_it_again() {
    let (e, clock) = timed();
    js_timed(&e);
    let root = load(&e, INTERRUPTED_TASK);
    let work = find(&e, root, "work");
    let mut s = scene(&e, root, 200.0, 200.0);
    later(&e, &clock, &mut s, Duration::ZERO);
    later(&e, &clock, &mut s, Duration::ZERO);
    let warned = e.take_warnings();
    assert_eq!(num(&e, root, "tries"), 2.0, "{warned:?} {:?}", faults(&e));
    assert_eq!(get(&e, work, "status").to_js_string().as_str(), "done");
    assert!(
        e.application_fault().is_none(),
        "{:?}",
        e.application_fault()
    );
}

/// Frames at one time until the scene rests, as a host that lets no time
/// pass does (a test settling a scene).
fn settle(e: &Engine, s: &mut ljos_quick::QuickScene, clock: &VirtualClock) {
    for _ in 0..64 {
        let now = clock.now();
        let out = s.frame(e, now);
        if !s.needs_frame() && out.next_wake.is_none_or(|t| t > now) && !e.has_due_calls() {
            return;
        }
    }
    panic!("the scene did not come to rest");
}

#[test]
fn content_made_again_at_once_needs_no_time_to_pass() {
    let (e, clock) = timed();
    js_timed(&e);
    let outer = load(&e, INTERRUPTED_CONTENT);
    let root = find(&e, outer, "root");
    let mut s = scene(&e, outer, 200.0, 200.0);
    settle(&e, &mut s, &clock);
    assert_eq!(num(&e, root, "tries"), 2.0, "{:?}", faults(&e));
}
