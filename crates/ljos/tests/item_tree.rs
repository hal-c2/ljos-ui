//! A test or a host finds an item by its `objectName`, popups in the
//! window's overlay included, and clicks the centre of where it is in the
//! scene.

use ljos::App;
use ljos::item::{self, PointerEvent, kurbo};
use ljos::prelude::{Engine, ObjectId, Value};
use ljos::testing::TestWindow;

const MAIN: &str = r#"
import QtQuick
import QtQuick.Controls

Item {
    width: 400; height: 300
    property string last: ""
    property point mapped: tilted.mapToItem(null, 10, 5)
    property bool open: false
    Column {
        x: 20; y: 10
        Button { objectName: "save"; text: "Save"; onClicked: last = "save" }
        Rectangle {
            id: tilted; objectName: "tilted"
            width: 40; height: 20; rotation: 30; scale: 1.5
        }
    }
    Popup {
        x: 150; y: 100; visible: open
        Button { objectName: "ok"; text: "OK"; onClicked: last = "ok" }
    }
}
"#;

fn open() -> (Engine, TestWindow, ObjectId) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), MAIN).unwrap();
    let source = ljos::dev::compile(dir.path(), None, ljos::dev::OnError::Fail).unwrap();
    let engine = App::new("item-tree").source(source).build_engine();
    let mut window = TestWindow::new(&engine, 400.0, 300.0);
    let root = engine.create_from_url("Main", None, Vec::new()).unwrap();
    window.show(root).unwrap();
    (engine, window, root)
}

/// The first item named `name` under `from`, depth first.
fn find(engine: &Engine, from: ObjectId, name: &str) -> Option<ObjectId> {
    if engine.read_by_name(from, "objectName") == Value::from(name) {
        return Some(from);
    }
    item::child_items(engine, from)
        .into_iter()
        .find_map(|c| find(engine, c, name))
}

/// Looks in the window's content, then in its overlay.
fn find_in_window(engine: &Engine, root: ObjectId, name: &str) -> Option<ObjectId> {
    find(engine, root, name).or_else(|| find(engine, item::overlay(engine, root)?, name))
}

fn click(window: &mut TestWindow, at: kurbo::Point) {
    let t = window.now();
    window.pointer(&PointerEvent::press(at.x, at.y, t));
    window.pointer(&PointerEvent::release(at.x, at.y, t));
    window.settle().unwrap();
}

#[test]
fn a_button_in_the_content_is_found_and_clicked() {
    let (engine, mut window, root) = open();
    let save = find_in_window(&engine, root, "save").expect("the save button");
    let rect = item::scene_rect(&engine, save);
    assert_eq!((rect.x0, rect.y0), (20.0, 10.0));
    click(&mut window, rect.center());
    assert_eq!(engine.read_by_name(root, "last"), Value::from("save"));
}

#[test]
fn a_button_in_an_open_popup_is_found_through_the_overlay() {
    let (engine, mut window, root) = open();
    engine
        .write_by_name(root, "open", Value::Bool(true))
        .unwrap();
    window.settle().unwrap();
    let ok = find_in_window(&engine, root, "ok").expect("the popup's button");
    assert!(
        find(&engine, root, "ok").is_none(),
        "not a child of the root"
    );
    let overlay = item::overlay(&engine, root).unwrap();
    assert_eq!(item::overlay(&engine, ok), Some(overlay));
    let rect = item::scene_rect(&engine, ok);
    assert!(rect.x0 > 150.0 && rect.y0 > 100.0, "{rect:?}");
    click(&mut window, rect.center());
    assert_eq!(engine.read_by_name(root, "last"), Value::from("ok"));
}

#[test]
fn mapping_to_the_scene_agrees_with_map_to_item_null() {
    let (engine, _window, root) = open();
    let tilted = find(&engine, root, "tilted").unwrap();
    let p = item::map_to_scene(&engine, tilted, kurbo::Point::new(10.0, 5.0));
    let Value::Point(q) = engine.read_by_name(root, "mapped") else {
        panic!("mapped is not a point");
    };
    assert!((p.x - q.x).abs() < 1e-9 && (p.y - q.y).abs() < 1e-9);
    let rect = item::scene_rect(&engine, tilted);
    assert!(rect.contains(p), "{rect:?} {p:?}");
    assert!(rect.width() > 40.0 * 1.5, "the box around the turned item");
}
