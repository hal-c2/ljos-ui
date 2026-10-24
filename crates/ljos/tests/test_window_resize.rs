//! A `TestWindow` changes size on the clock it has: what is bound to the
//! window's width follows, and a timer started before keeps its time.

use ljos::App;
use ljos::prelude::{Engine, ObjectId, Value};
use ljos::testing::TestWindow;

fn open(main: &str) -> (Engine, TestWindow, ObjectId) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), main).unwrap();
    let source = ljos::dev::compile(dir.path(), None, ljos::dev::OnError::Fail).unwrap();
    let engine = App::new("resize").source(source).build_engine();
    let mut window = TestWindow::new(&engine, 800.0, 600.0);
    let root = engine.create_from_url("Main", None, Vec::new()).unwrap();
    window.show(root).unwrap();
    (engine, window, root)
}

fn number(engine: &Engine, root: ObjectId, name: &str) -> f64 {
    match engine.read_by_name(root, name) {
        Value::Int(n) => n.into(),
        Value::Real(n) => n,
        other => panic!("{name} is not a number: {other:?}"),
    }
}

#[test]
fn a_window_root_takes_the_new_size() {
    let (engine, mut window, root) = open(
        r#"
import QtQuick
import QtQuick.Window
Window {
    width: 800; height: 600
    property alias side: side.width
    property bool narrow: width < 500
    Rectangle { id: side; width: parent.width / 4; height: 10 }
}
"#,
    );
    assert_eq!(number(&engine, root, "side"), 200.0);
    window.resize(400.0, 300.0).unwrap();
    assert_eq!(number(&engine, root, "side"), 100.0);
    assert_eq!(engine.read_by_name(root, "narrow"), Value::Bool(true));
}

#[test]
fn an_item_root_takes_the_new_size_on_the_same_clock() {
    let (engine, mut window, root) = open(
        r#"
import QtQuick
Item {
    width: 800; height: 600
    property alias side: side.width
    property int ticks: 0
    Timer { interval: 100; running: true; onTriggered: parent.ticks++ }
    Rectangle { id: side; width: parent.width / 4; height: 10 }
}
"#,
    );
    let before = window.now();
    window.advance(60).unwrap();
    window.resize(400.0, 300.0).unwrap();
    assert_eq!(number(&engine, root, "side"), 100.0);
    assert_eq!(number(&engine, root, "height"), 300.0);
    assert_eq!(window.now(), before + std::time::Duration::from_millis(60));
    window.advance(40).unwrap();
    assert_eq!(number(&engine, root, "ticks"), 1.0);
}
