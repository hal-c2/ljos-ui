//! Items that run out of script heap together recover through one
//! collection, whatever kind of item they are (ljos-ui#678): a column of
//! boundaries failing on one full heap must not pause the app once per
//! boundary. A heap that runs out again after that collection is collected
//! for again.

mod common;

use common::*;
use ljos_engine::{Engine, ObjectId};
use ljos_runtime::Fault;

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
