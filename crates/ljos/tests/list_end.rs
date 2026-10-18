//! A list at its end, sent there again: a row the view measured keeps
//! its height in the length estimate, so `positionViewAtEnd()` with
//! nothing changed leaves `contentHeight` alone, and a view that keeps
//! to its end through `Qt.callLater` settles.

use ljos::App;
use ljos::prelude::{Engine, ObjectId, Value};
use ljos::testing::TestWindow;

/// 390 rows of fixed, different heights; `grow()` makes the last one
/// 320 px tall instead of 40.
const ROWS: &str = r#"
        topMargin: 12
        bottomMargin: 12
        model: 390
        delegate: Rectangle {
            required property int index
            width: ListView.view.width
            height: index === 389 ? (win.grown ? 320 : 40)
                  : index === 0 ? 198
                  : index >= 382 ? win.last[index - 382]
                  : 60 + (index * 37) % 300
        }
"#;

fn open(body: &str) -> (Engine, TestWindow, ObjectId) {
    let qml = format!(
        r#"
import QtQuick
Item {{
    id: win
    width: 900
    height: 758
    property bool grown: false
    readonly property var last: [246, 48, 42, 246, 48, 42, 370]
    function grow() {{ grown = true }}
    {body}
}}
"#
    )
    .replace("ROWS", ROWS);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), qml).unwrap();
    let source = ljos::dev::compile(dir.path(), None, ljos::dev::OnError::Fail).unwrap();
    let engine = App::new("list-end").source(source).build_engine();
    let mut window = TestWindow::new(&engine, 900.0, 758.0);
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

fn call(engine: &Engine, root: ObjectId, name: &str) {
    engine.invoke_by_name(root, name, &[]).unwrap();
}

#[test]
fn a_list_at_its_end_keeps_its_length_when_sent_there_again() {
    let (engine, mut window, root) = open(
        r#"
    property int changes: 0
    property real length: view.contentHeight
    function toEnd() { view.positionViewAtEnd() }
    ListView {
        id: view
        anchors.fill: parent
        onContentHeightChanged: win.changes++
        ROWS
    }
"#,
    );
    for grown in [false, true] {
        if grown {
            call(&engine, root, "grow");
        }
        for _ in 0..5 {
            call(&engine, root, "toEnd");
            window.advance(16).unwrap();
        }
        let (changes, length) = (
            number(&engine, root, "changes"),
            number(&engine, root, "length"),
        );
        for _ in 0..3 {
            call(&engine, root, "toEnd");
            window.advance(16).unwrap();
        }
        assert_eq!(
            number(&engine, root, "changes") - changes,
            0.0,
            "grown {grown}: contentHeight {length} -> {}",
            number(&engine, root, "length")
        );
    }
}

#[test]
fn a_list_that_follows_its_end_settles_after_its_last_row_grows() {
    let (engine, mut window, root) = open(
        r#"
    property int calls: 0
    property bool looped: false
    ListView {
        id: view
        anchors.fill: parent
        function stick() {
            if (++win.calls > 1000) {
                win.looped = true;
                return;
            }
            positionViewAtEnd();
        }
        onContentHeightChanged: Qt.callLater(stick)
        Component.onCompleted: Qt.callLater(stick)
        ROWS
    }
"#,
    );
    for frame in 0..10 {
        if frame == 5 {
            call(&engine, root, "grow");
        }
        window.advance(16).unwrap();
    }
    let settled = number(&engine, root, "calls");
    window.advance(160).unwrap();
    assert_eq!(engine.read_by_name(root, "looped"), Value::Bool(false));
    assert_eq!(
        number(&engine, root, "calls"),
        settled,
        "it still calls itself"
    );
}
