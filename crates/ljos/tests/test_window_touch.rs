//! A test of an application of the facade alone puts a finger on its
//! window (`TestWindow::touch`), and tells a finger from a mouse by the
//! types `ljos::item` names: what refuses the mouse
//! (`MultiPointTouchArea { mouseEnabled: false }`) still takes the finger.

use ljos::item::{PointerEvent, PointerId, PointerSource, TouchEvent, TouchPhase, kurbo};
use ljos::prelude::*;
use ljos::testing::TestWindow;

const MAIN: &str = r#"
import QtQuick
Item {
    id: root
    width: 100; height: 100
    property int pressed: 0
    property int released: 0
    MultiPointTouchArea {
        anchors.fill: parent
        mouseEnabled: false
        onPressed: root.pressed += 1
        onReleased: root.released += 1
    }
}
"#;

fn load(engine: &Engine) -> ObjectId {
    use ljos_compiler::{Project, compile_file};
    let mut project = Project::new(qml_front::QtTypeOracle);
    project.add("Main", MAIN);
    let c = compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
    assert!(!c.has_errors(), "Main: {:?}", c.diagnostics);
    let mut source = ljos::MapSource::new();
    source.insert("Main", c.file);
    engine.set_component_source(source);
    engine.create_from_url("Main", None, Vec::new()).unwrap()
}

fn finger(phase: TouchPhase, x: f64, y: f64, window: &TestWindow) -> TouchEvent {
    TouchEvent {
        id: PointerId(7),
        phase,
        pos: kurbo::Point::new(x, y),
        time: window.now(),
    }
}

#[test]
fn a_finger_reaches_an_area_that_refuses_the_mouse() {
    let engine = ljos::engine();
    let mut window = TestWindow::new(&engine, 100.0, 100.0);
    let root = load(&engine);
    window.show(root).unwrap();

    let t = window.now();
    window.pointer(&PointerEvent::press(50.0, 50.0, t));
    window.pointer(&PointerEvent::release(50.0, 50.0, t));
    assert_eq!(engine.read_by_name(root, "pressed").to_number(), 0.0);

    assert!(window.touch(&[finger(TouchPhase::Begin, 50.0, 50.0, &window)]));
    window.touch(&[finger(TouchPhase::End, 50.0, 50.0, &window)]);
    assert_eq!(engine.read_by_name(root, "pressed").to_number(), 1.0);
    assert_eq!(engine.read_by_name(root, "released").to_number(), 1.0);
}

#[test]
fn a_pointer_event_says_it_came_from_a_finger() {
    let t = ljos::testing::TestWindow::new(&ljos::engine(), 10.0, 10.0).now();
    let ev = PointerEvent::press(1.0, 1.0, t).with_source(PointerSource::FINGER);
    assert_eq!(ev.source, PointerSource::FINGER);
    assert_ne!(PointerSource::FINGER, PointerSource::MOUSE);
}
