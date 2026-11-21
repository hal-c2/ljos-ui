//! What a full script heap, an interrupted script or a panic in native
//! work stops comes back on its own once the fault is over: a Repeater's
//! row, a view's delegate, a singleton.

mod common;

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::QuickScene;
use ljos_runtime::{Clock, VirtualClock};

fn timed() -> (Engine, Arc<VirtualClock>) {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    (e, clock)
}

/// A file whose item needs its script while it is made, and the script
/// takes room to load: tens of kilobytes, against the few left free.
fn row() -> String {
    let mut s = String::from("import QtQuick\nItem {\n    width: f0(1, 2).length; height: 10\n");
    for i in 0..40 {
        s.push_str(&format!(
            "    function f{i}(a, b) {{ const t = [a, b, \"row {i}\"]; return t.map(x => String(x) + {i}).join(\"-\") }}\n"
        ));
    }
    s.push_str("}\n");
    s
}

const REPEATER: &str = r#"
import QtQuick
Item {
    id: root
    objectName: "root"
    property bool on: false
    Column {
        objectName: "column"
        Repeater { objectName: "repeater"; model: root.on ? 3 : 0; delegate: Line {} }
    }
    function litter(n) {
        for (let i = 0; i < n; i++) { const a = { b: [i, i + 1, "x" + i] }; a.self = a }
    }
}
"#;

/// Starts the script engine and leaves the heap `room` bytes from full.
fn fill(e: &Engine, root: ObjectId, room: usize) {
    e.invoke_by_name(root, "litter", &[Value::from(0.0)])
        .unwrap();
    assert!(e.collect_garbage());
    let held = e.script_memory().unwrap().bytes;
    assert!(e.set_script_heap_limit(Some(held + room)));
}

fn widths(e: &Engine, repeater: ObjectId) -> Vec<f64> {
    (0..3)
        .map(|i| {
            e.invoke_by_name(repeater, "itemAt", &[Value::from(f64::from(i))])
                .ok()
                .and_then(|v| v.as_object())
                .map_or(-1.0, |o| num(e, o, "width"))
        })
        .collect()
}

#[test]
fn repeater_rows_tried_again_on_a_full_heap_collect_once_a_round() {
    let (e, clock) = timed();
    // Its script runs while it is made: the Repeater makes the row again
    // in its own timer, which runs out again before the next row's timer.
    let line = row().replace(
        "    width: f0(1, 2).length; height: 10\n",
        "    height: 10\n    Component.onCompleted: width = f0(1, 2).length\n",
    );
    compile(&e, &[("Main", REPEATER), ("Line", &line)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let repeater = find(&e, root, "repeater");
    fill(&e, root, 4 * 1024);
    set(&e, root, "on", Value::Bool(true));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    assert_eq!(widths(&e, repeater), [-1.0, -1.0, -1.0], "no row is made");
    // Every row is tried again together, and each try runs out again:
    // the first one's collection covers the rest, whose faults came
    // before it, round after round.
    for _ in 0..2 {
        let collections = e.script_memory().unwrap().collections;
        let due = e.next_timer_deadline().expect("the rows are tried again");
        clock.set(due);
        e.advance_timers(clock.now());
        frame(&mut s, &e);
        assert_eq!(widths(&e, repeater), [-1.0, -1.0, -1.0], "no row is made");
        assert_eq!(
            e.script_memory().unwrap().collections,
            collections + 1,
            "{:?}",
            e.take_warnings()
        );
    }
    // No retry went without the collection it needed: with room, they all
    // come back on the next round.
    assert!(e.set_script_heap_limit(None));
    let due = e.next_timer_deadline().expect("the rows are tried again");
    clock.set(due);
    e.advance_timers(clock.now());
    frame(&mut s, &e);
    assert_eq!(widths(&e, repeater), [12.0, 12.0, 12.0]);
}

#[test]
fn a_repeater_row_whose_script_ran_out_of_heap_comes_back_once_there_is_room() {
    let (e, clock) = timed();
    compile(&e, &[("Main", REPEATER), ("Line", &row())]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let repeater = find(&e, root, "repeater");
    fill(&e, root, 4 * 1024);
    set(&e, root, "on", Value::Bool(true));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    // The rows are made, but their script did not load.
    assert_eq!(widths(&e, repeater), [0.0, 0.0, 0.0]);
    let warned = e.take_warnings();
    assert!(
        warned.iter().any(|w| w.contains("JS heap limit exceeded")),
        "{warned:?}"
    );
    // While the heap stays full, each try runs out again, at a bounded rate.
    let due = e.next_timer_deadline().expect("the rows are tried again");
    clock.set(due);
    e.advance_timers(clock.now());
    frame(&mut s, &e);
    assert_eq!(widths(&e, repeater), [0.0, 0.0, 0.0]);
    let due = e.next_timer_deadline().expect("and tried again after that");
    let wait = due.saturating_since(clock.now());
    assert!(
        wait >= Duration::from_secs(1) && wait <= Duration::from_secs(60),
        "{wait:?}"
    );
    // Whatever filled the heap lets go: the rows load their script, with
    // nobody changing the model.
    assert!(e.set_script_heap_limit(None));
    clock.set(due);
    e.advance_timers(clock.now());
    frame(&mut s, &e);
    assert_eq!(widths(&e, repeater), [12.0, 12.0, 12.0]);
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
}

const LIST_VIEW: &str = r#"
import QtQuick
Item {
    id: root
    objectName: "root"
    property bool on: false
    ListView {
        objectName: "view"
        width: 100; height: 100
        model: root.on ? 3 : 0
        delegate: Line {}
    }
    function litter(n) {
        for (let i = 0; i < n; i++) { const a = { b: [i, i + 1, "x" + i] }; a.self = a }
    }
}
"#;

fn delegate_widths(e: &Engine, view: ObjectId) -> Vec<f64> {
    (0..3)
        .map(|i| {
            e.invoke_by_name(view, "itemAtIndex", &[Value::from(f64::from(i))])
                .ok()
                .and_then(|v| v.as_object())
                .map_or(-1.0, |o| num(e, o, "width"))
        })
        .collect()
}

/// Lets the timers due by the next deadline run, and draws a frame.
fn wait_out(e: &Engine, clock: &VirtualClock, s: &mut QuickScene) -> Duration {
    let due = e.next_timer_deadline().expect("it is tried again");
    let wait = due.saturating_since(clock.now());
    clock.set(due);
    e.advance_timers(clock.now());
    frame(s, e);
    wait
}

#[test]
fn a_list_view_delegate_whose_script_ran_out_of_heap_comes_back_once_there_is_room() {
    let (e, clock) = timed();
    compile(&e, &[("Main", LIST_VIEW), ("Line", &row())]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let view = find(&e, root, "view");
    fill(&e, root, 4 * 1024);
    set(&e, root, "on", Value::Bool(true));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    let before = delegate_widths(&e, view);
    assert!(before.iter().all(|w| *w != 12.0), "{before:?}");
    // Still full: tried again, at a bounded rate.
    let wait = wait_out(&e, &clock, &mut s);
    assert!(wait <= Duration::from_secs(60), "{wait:?}");
    assert!(e.set_script_heap_limit(None));
    let wait = wait_out(&e, &clock, &mut s);
    assert!(wait <= Duration::from_secs(60), "{wait:?}");
    assert_eq!(delegate_widths(&e, view), [12.0, 12.0, 12.0]);
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
}

const READS_THEME: &str = r#"
import QtQuick
Item {
    id: root
    objectName: "root"
    property bool on: false
    property int w: root.on ? Theme.w : -1
    function litter(n) {
        for (let i = 0; i < n; i++) { const a = { b: [i, i + 1, "x" + i] }; a.self = a }
    }
}
"#;

#[test]
fn a_singleton_whose_script_ran_out_of_heap_comes_back_once_there_is_room() {
    let (e, clock) = timed();
    let theme = format!(
        "pragma Singleton\n{}",
        row()
            .replace("Item {", "QtObject {\n    property int w: f0(1, 2).length")
            .replace("    width: f0(1, 2).length; height: 10\n", "")
    );
    compile(&e, &[("Main", READS_THEME), ("Theme", &theme)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    fill(&e, root, 4 * 1024);
    set(&e, root, "on", Value::Bool(true));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    assert_ne!(num(&e, root, "w"), 12.0);
    let warned = e.take_warnings();
    assert!(
        warned.iter().any(|w| w.contains("JS heap limit exceeded")),
        "{warned:?}"
    );
    let wait = wait_out(&e, &clock, &mut s);
    assert!(wait <= Duration::from_secs(60), "{wait:?}");
    assert_ne!(num(&e, root, "w"), 12.0);
    assert!(e.set_script_heap_limit(None));
    let wait = wait_out(&e, &clock, &mut s);
    assert!(wait <= Duration::from_secs(60), "{wait:?}");
    assert_eq!(num(&e, root, "w"), 12.0, "the singleton is there again");
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
}

#[test]
fn a_singleton_that_ran_out_of_heap_while_it_was_made_is_made_again() {
    let (e, clock) = timed();
    // Its script runs while it is made, not when a binding is first read.
    let theme = format!(
        "pragma Singleton\n{}",
        row()
            .replace(
                "Item {",
                "QtObject {\n    property int w: 0\n    Component.onCompleted: w = f0(1, 2).length",
            )
            .replace("    width: f0(1, 2).length; height: 10\n", "")
    );
    compile(&e, &[("Main", READS_THEME), ("Theme", &theme)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    fill(&e, root, 4 * 1024);
    set(&e, root, "on", Value::Bool(true));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    assert_ne!(num(&e, root, "w"), 12.0);
    let warned = e.take_warnings();
    assert!(
        warned
            .iter()
            .any(|w| w.contains("the singleton Theme is not available")
                && w.contains("making it again")),
        "{warned:?}"
    );
    let wait = wait_out(&e, &clock, &mut s);
    assert!(wait <= Duration::from_secs(60), "{wait:?}");
    assert_ne!(num(&e, root, "w"), 12.0);
    assert!(e.set_script_heap_limit(None));
    while e.next_timer_deadline().is_some() {
        let wait = wait_out(&e, &clock, &mut s);
        assert!(wait <= Duration::from_secs(60), "{wait:?}");
        if num(&e, root, "w") == 12.0 {
            break;
        }
    }
    assert_eq!(num(&e, root, "w"), 12.0, "the singleton is there again");
    frame(&mut s, &e);
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
}

#[test]
fn a_list_view_delegate_that_ran_out_of_heap_while_it_was_made_is_made_again() {
    let (e, clock) = timed();
    // Its script runs while it is made, not when a binding is first read.
    let line = row().replace(
        "    width: f0(1, 2).length; height: 10\n",
        "    height: 10\n    Component.onCompleted: width = f0(1, 2).length\n",
    );
    compile(&e, &[("Main", LIST_VIEW), ("Line", &line)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let view = find(&e, root, "view");
    fill(&e, root, 4 * 1024);
    set(&e, root, "on", Value::Bool(true));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    let before = delegate_widths(&e, view);
    assert!(before.iter().all(|w| *w != 12.0), "{before:?}");
    let wait = wait_out(&e, &clock, &mut s);
    assert!(wait <= Duration::from_secs(60), "{wait:?}");
    assert!(e.set_script_heap_limit(None));
    while e.next_timer_deadline().is_some() && delegate_widths(&e, view) != [12.0; 3] {
        let wait = wait_out(&e, &clock, &mut s);
        assert!(wait <= Duration::from_secs(60), "{wait:?}");
    }
    assert_eq!(delegate_widths(&e, view), [12.0, 12.0, 12.0]);
    frame(&mut s, &e);
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
}

const SLOW_BINDING: &str = r#"
import QtQuick
Item {
    id: root
    objectName: "root"
    property bool on: false
    property int made: 0
    readonly property var tries: ({ binding: 0, handler: 0 })
    // Runs over its time the first three times it is called for `what`.
    function slow(what) {
        if (tries[what]++ < 3) { for (;;) {} }
        return 5
    }
    ListView {
        objectName: "view"
        width: 100; height: 100
        model: root.on ? 3 : 0
        delegate: Item {
            width: 12; height: 10
            property int v: root.slow("binding")
            // Runs over its time while the delegate is made.
            Component.onCompleted: { root.made++; root.slow("handler") }
        }
    }
}
"#;

#[test]
fn a_list_view_delegate_whose_scripts_ran_over_their_time_is_shown_and_its_binding_runs_again() {
    let (e, clock) = timed();
    compile(&e, &[("Main", SLOW_BINDING)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let view = find(&e, root, "view");
    e.set_js_dispatch_budget(Duration::from_millis(10), Duration::from_millis(50))
        .unwrap();
    set(&e, root, "on", Value::Bool(true));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    // Made once and shown, though a script was stopped while it was made
    // and its binding was stopped after.
    assert_eq!(num(&e, root, "made"), 3.0);
    assert_eq!(delegate_widths(&e, view), [12.0, 12.0, 12.0]);
    let v = |e: &Engine| -> Vec<f64> {
        (0..3)
            .map(|i| {
                e.invoke_by_name(view, "itemAtIndex", &[Value::from(f64::from(i))])
                    .ok()
                    .and_then(|v| v.as_object())
                    .map_or(-1.0, |o| num(e, o, "v"))
            })
            .collect()
    };
    assert_eq!(v(&e), [0.0, 0.0, 0.0]);
    // The binding runs again after a wait; the delegate is not made again.
    let wait = wait_out(&e, &clock, &mut s);
    assert!(wait <= Duration::from_secs(60), "{wait:?}");
    assert_eq!(v(&e), [5.0, 5.0, 5.0]);
    assert_eq!(num(&e, root, "made"), 3.0);
    frame(&mut s, &e);
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
}
