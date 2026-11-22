//! A `ShellLoader` asked to load again while its source's shell is being
//! made: the shell being made already reads the source as it is, so it is
//! made once, whether a host takes the ask or none does.

mod common;

use std::sync::Arc;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::{Reread, Rereads};

const MAIN: &str = r#"
import QtQuick
import Ljos.Blocks
Item {
    id: root
    property bool on: false
    property int made: 0
    ShellLoader {
        id: loader
        objectName: "loader"
        source: root.on ? "Again" : ""
        fallback: "Light"
    }
}
"#;

/// A shell that asks its loader to load again as it completes; a bounded
/// number of times, so that a loader that makes it again each time ends.
const AGAIN: &str = r#"
import QtQuick
Item {
    objectName: "again"
    Component.onCompleted: { root.made++; if (root.made < 3) loader.reload() }
}
"#;

const LIGHT: &str = r#"
import QtQuick
Item { objectName: "light" }
"#;

fn started() -> (Engine, ObjectId, ObjectId) {
    let e = engine();
    compile(&e, &[("Main", MAIN), ("Again", AGAIN), ("Light", LIGHT)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let loader = find(&e, root, "loader");
    (e, root, loader)
}

fn settle(e: &Engine) {
    e.run_pending_calls();
    e.deliver_faults();
    e.process_deferred_deletes();
}

fn shown(e: &Engine, loader: ObjectId) -> String {
    get(e, loader, "item")
        .as_object()
        .map(|o| get(e, o, "objectName").to_js_string().to_string())
        .unwrap_or_default()
}

#[test]
fn a_shell_that_asks_its_loader_to_load_again_as_it_is_made_is_made_once_where_no_host_reads() {
    let (e, root, loader) = started();
    settle(&e);
    assert_eq!(
        num(&e, loader, "status") as i32,
        2,
        "the fallback stands in"
    );
    set(&e, root, "on", Value::Bool(true));
    settle(&e);
    assert_eq!(
        num(&e, root, "made") as i32,
        1,
        "the shell being made reads the source already"
    );
    assert_eq!(num(&e, loader, "status") as i32, 1);
    assert_eq!(shown(&e, loader), "again");
}

#[test]
fn a_shell_that_asks_its_loader_to_load_again_as_it_is_made_is_made_once_where_a_host_reads() {
    let (e, root, loader) = started();
    let rereads = Rereads::of(&e);
    rereads.serve(Arc::new(|| {}));
    settle(&e);
    set(&e, root, "on", Value::Bool(true));
    settle(&e);
    // The host reads the files at its next frame, and finds them as they
    // were.
    assert!(rereads.take_asked(), "the shell asked the host");
    rereads.answered(&e, Reread::Unchanged);
    settle(&e);
    assert_eq!(
        num(&e, root, "made") as i32,
        1,
        "the shell shown is kept: its files are as they were"
    );
    assert_eq!(num(&e, loader, "status") as i32, 1);
    assert_eq!(shown(&e, loader), "again");
}
