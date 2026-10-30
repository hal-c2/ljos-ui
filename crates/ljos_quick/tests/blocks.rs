//! `Ljos.Blocks`: what a Slot shows of the contributions alive for its
//! name, and how a ShellLoader falls back.

mod common;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};

fn alive_named(e: &Engine, root: ObjectId, name: &str) -> bool {
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        if !e.is_alive(o) {
            continue;
        }
        if e.read_by_name(o, "objectName").to_js_string().as_str() == name {
            return true;
        }
        stack.extend(e.children(o));
    }
    false
}

/// What a frame does after a turn: deletes what was destroyed and hands
/// faults to their boundaries.
fn settle(e: &Engine) {
    e.deliver_faults();
    e.process_deferred_deletes();
}

const PLUGIN: &str = r#"
import QtQuick
import Ljos.Blocks
Item {
    Contribution { slot: "tools"; mode: Contribution.Replace
        Rectangle { objectName: "plugged"; width: 10; height: 10 } }
}
"#;

const HOST: &str = r#"
import QtQuick
import Ljos.Blocks
Item {
    id: root
    objectName: "root"
    property bool plugin: false
    Row {
        objectName: "row"
        Slot { objectName: "slot"; name: "tools"
            Rectangle { objectName: "own"; width: 10; height: 10 } }
    }
    Loader { objectName: "loader"; active: root.plugin; source: "Plugin.qml" }
}
"#;

fn host() -> (Engine, ObjectId) {
    let e = engine();
    compile(&e, &[("Main", HOST), ("Plugin", PLUGIN)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    (e, root)
}

#[test]
fn a_contribution_unloaded_takes_its_item_and_gives_the_slot_back_its_own() {
    let (e, root) = host();
    let slot = find(&e, root, "slot");
    assert!(alive_named(&e, root, "own"));
    set(&e, root, "plugin", Value::Bool(true));
    settle(&e);
    assert!(alive_named(&e, root, "plugged"));
    assert!(!alive_named(&e, root, "own"));
    set(&e, root, "plugin", Value::Bool(false));
    settle(&e);
    settle(&e);
    assert!(!alive_named(&e, root, "plugged"));
    assert!(alive_named(&e, root, "own"));
    assert_eq!(num(&e, slot, "count"), 1.0);
}

#[test]
fn contributed_items_sit_before_the_slot_in_its_parent() {
    let (e, root) = host();
    set(&e, root, "plugin", Value::Bool(true));
    settle(&e);
    let row = find(&e, root, "row");
    let names: Vec<String> = list(&e, row, "children")
        .into_iter()
        .map(|c| e.read_by_name(c, "objectName").to_js_string().to_string())
        .collect();
    assert_eq!(names, ["plugged", "slot"]);
}
