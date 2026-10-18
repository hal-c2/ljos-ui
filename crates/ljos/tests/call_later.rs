//! `Qt.callLater` as a test window drives it: a call queued while the
//! queue drains waits for the next pass, as on Qt, so a function that
//! queues itself again lets the frame end.

use ljos::App;
use ljos::prelude::{Engine, ObjectId, Value};
use ljos::testing::TestWindow;

fn open(qml: &str) -> (Engine, TestWindow, ObjectId) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), qml).unwrap();
    let source = ljos::dev::compile(dir.path(), None, ljos::dev::OnError::Fail).unwrap();
    let engine = App::new("call-later").source(source).build_engine();
    let mut window = TestWindow::new(&engine, 200.0, 200.0);
    let root = engine.create_from_url("Main", None, Vec::new()).unwrap();
    window.show(root).unwrap();
    (engine, window, root)
}

fn int(engine: &Engine, root: ObjectId, name: &str) -> i64 {
    match engine.read_by_name(root, name) {
        Value::Int(n) => n.into(),
        Value::Real(n) => n as i64,
        other => panic!("{name} is not a number: {other:?}"),
    }
}

#[test]
fn a_call_queued_by_a_queued_call_waits_for_the_next_frame() {
    let (engine, mut window, root) = open(
        r#"
import QtQuick
Item {
    property int runs: 0
    function again() { runs++; if (runs < 100000) Qt.callLater(again) }
    function start() { Qt.callLater(again) }
}
"#,
    );
    engine.invoke_by_name(root, "start", &[]).unwrap();
    window.advance(16).unwrap();
    let first = int(&engine, root, "runs");
    assert!((1..=2).contains(&first), "one frame ran it {first} times");
    // It goes on running, a frame at a time.
    window.advance(160).unwrap();
    let later = int(&engine, root, "runs");
    assert!(
        later > first && later <= first + 12,
        "ten frames ran it {} times",
        later - first
    );
}

#[test]
fn the_same_function_queued_twice_in_one_pass_runs_once() {
    let (engine, mut window, root) = open(
        r#"
import QtQuick
Item {
    property int runs: 0
    function count() { runs++ }
    function start() { Qt.callLater(count); Qt.callLater(count) }
}
"#,
    );
    engine.invoke_by_name(root, "start", &[]).unwrap();
    window.advance(100).unwrap();
    assert_eq!(int(&engine, root, "runs"), 1);
}
