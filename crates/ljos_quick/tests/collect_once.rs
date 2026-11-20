//! Items that run out of script heap together recover through one
//! collection, whatever kind of item they are (ljos-ui#678): a column of
//! boundaries failing on one full heap must not pause the app once per
//! boundary. A heap that runs out again after that collection is collected
//! for again.

mod common;

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_runtime::{Clock, Fault, VirtualClock};

const MAIN: &str = r#"
import QtQuick
import Ljos
import Ljos.Blocks
Item {
    Component { id: row; Item { objectName: "cell" } }
    ErrorBoundary { objectName: "b0"; Item { objectName: "content" } }
    ErrorBoundary { objectName: "b1"; Item { objectName: "content" } }
    ErrorBoundary { objectName: "b2"; Item { objectName: "content" } }
    ShellLoader { objectName: "s0"; source: "Light" }
    ShellLoader { objectName: "s1"; source: "Light" }
    Loader { objectName: "l0"; sourceComponent: row }
    Loader { objectName: "l1"; sourceComponent: row }
    function warm() { return 1 }
}
"#;

const LIGHT: &str = r#"
import QtQuick
Item { objectName: "light" }
"#;

fn collections(e: &Engine) -> u64 {
    e.script_memory().unwrap().collections
}

/// What each item named in `names` shows.
fn shown(e: &Engine, root: ObjectId, names: &[&str]) -> Vec<ObjectId> {
    names
        .iter()
        .map(|n| {
            get(e, find(e, root, n), "item")
                .as_object()
                .unwrap_or_else(|| panic!("{n} shows nothing"))
        })
        .collect()
}

#[test]
fn boundaries_shells_and_loaders_out_of_heap_together_collect_once() {
    let e = engine();
    compile(&e, &[("Main", MAIN), ("Light", LIGHT)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    // Running a script starts the script engine.
    e.invoke_by_name(root, "warm", &[]).unwrap();
    let names = ["b0", "b1", "b2", "s0", "s1", "l0", "l1"];
    let items = shown(&e, root, &names);

    for item in &items {
        e.report_fault(Some(*item), Fault::JsOutOfMemory);
    }
    let before = collections(&e);
    e.deliver_faults();
    e.process_deferred_deletes();
    assert_eq!(collections(&e), before + 1, "{:?}", e.take_warnings());

    // Out of heap again after that collection: collected for again, once.
    // Loaders, because they load again at once, where a boundary waits
    // for its retry.
    for item in shown(&e, root, &["l0", "l1"]) {
        e.report_fault(Some(item), Fault::JsOutOfMemory);
    }
    e.deliver_faults();
    e.process_deferred_deletes();
    assert_eq!(collections(&e), before + 2);
}

#[test]
fn a_collection_since_the_heap_ran_out_covers_it() {
    let e = engine();
    compile(&e, &[("Main", MAIN), ("Light", LIGHT)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    e.invoke_by_name(root, "warm", &[]).unwrap();
    let b0 = shown(&e, root, &["b0"])[0];
    e.report_fault(Some(b0), Fault::JsOutOfMemory);
    // Someone collected meanwhile (a tool, memory pressure): the boundary
    // has nothing left to free.
    assert!(e.collect_garbage());
    let before = collections(&e);
    e.deliver_faults();
    assert_eq!(collections(&e), before);
}

/// Rows whose script fills the heap whenever `hog` is set: every reload
/// runs out again.
const HOGS: &str = r#"
import QtQuick
Item {
    id: root
    property bool hog: false
    Component {
        id: row
        Item {
            objectName: "cell"
            Component.onCompleted: if (root.hog) { let h = []; for (;;) h.push(new Array(100).fill(1)) }
        }
    }
    Loader { objectName: "l0"; sourceComponent: row }
    Loader { objectName: "l1"; sourceComponent: row }
    Loader { objectName: "l2"; sourceComponent: row }
    function warm() { return 1 }
}
"#;

#[test]
fn loaders_whose_reloads_run_out_again_collect_once_per_round() {
    let e = engine();
    compile(&e, &[("Main", HOGS)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    e.invoke_by_name(root, "warm", &[]).unwrap();
    let items = shown(&e, root, &["l0", "l1", "l2"]);
    assert!(e.collect_garbage());
    let held = e.script_memory().unwrap().bytes;
    assert!(e.set_script_heap_limit(Some(held + (4 << 20))));
    set(&e, root, "hog", Value::Bool(true));

    for item in &items {
        e.report_fault(Some(*item), Fault::JsOutOfMemory);
    }
    let before = collections(&e);
    e.deliver_faults();
    e.deliver_faults();
    e.process_deferred_deletes();
    // l0's collection came after every first fault, so l1 and l2 reload
    // into it; their reloads, which ran out again, came before l0's
    // second fault, and its collection covers them.
    let ran_out = e
        .faults()
        .iter()
        .filter(|f| matches!(f.fault, Fault::JsOutOfMemory))
        .count();
    assert!(ran_out >= 6, "every reload ran out: {:?}", e.faults());
    assert_eq!(collections(&e), before + 2, "{:?}", e.take_warnings());
    assert!(e.set_script_heap_limit(None));
}

const TASKS: &str = r#"
import QtQuick
import Ljos.Core
Item {
    id: root
    property int n: 21
    property bool spin: false
    Task {
        id: task
        objectName: "task"
        args: [root.n, root.spin]
        run: (n: int, spin: bool): int => { if (spin) for (;;) {} return n * 2 }
    }
    property string status: task.status
}
"#;

/// Runs what is due now: promise jobs and calls.
fn settle(e: &Engine, clock: &VirtualClock) {
    e.advance_timers(clock.now());
    e.run_pending_calls();
    e.deliver_faults();
}

fn tasks(e: &Engine) -> (Arc<VirtualClock>, ObjectId) {
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    compile(e, &[("Main", TASKS)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    settle(e, &clock);
    assert_eq!(get(e, root, "status").to_js_string().to_string(), "done");
    (clock, root)
}

#[test]
fn a_task_that_could_not_start_on_a_full_heap_collects() {
    let e = engine();
    let (clock, root) = tasks(&e);
    assert!(e.collect_garbage());
    let held = e.script_memory().unwrap().bytes;
    assert!(e.set_script_heap_limit(Some(held + 256)));
    let before = collections(&e);
    set(&e, root, "n", Value::from(5.0));
    settle(&e, &clock);
    assert_eq!(get(&e, root, "status").to_js_string().to_string(), "failed");
    // A start that waits 0 s starts again within the same settle, and
    // runs out again: each start is its own round, and collects once.
    let warned = e.take_warnings();
    let starts = warned
        .iter()
        .filter(|w| w.contains("could not start"))
        .count();
    assert!(starts >= 1, "{warned:?}");
    assert_eq!(collections(&e), before + starts as u64, "{warned:?}");
    assert!(e.set_script_heap_limit(None));
}

#[test]
fn a_task_whose_run_was_interrupted_does_not_collect() {
    let e = engine();
    let (clock, root) = tasks(&e);
    e.set_js_dispatch_budget(Duration::from_millis(10), Duration::from_millis(50))
        .unwrap();
    let before = collections(&e);
    set(&e, root, "spin", Value::Bool(true));
    settle(&e, &clock);
    let warned = e.take_warnings();
    assert!(
        warned.iter().any(|w| w.contains("interrupted")),
        "{warned:?}"
    );
    assert_eq!(collections(&e), before, "{warned:?}");
}
