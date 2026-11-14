//! An application posts input to a window it shows and watches an item get
//! it (`ljos::input`); what a driver gives (a test window, an agent's
//! inspector) is told apart from the user's.

use std::cell::RefCell;
use std::rc::Rc;

use ljos::input::{Input, InputOrigin, PointerEvent, post_input, watch_input};
use ljos::prelude::*;
use ljos::testing::TestWindow;

const MAIN: &str = r#"
import QtQuick
Item {
    id: root
    width: 100; height: 100
    property int clicks: 0
    MouseArea {
        objectName: "area"
        anchors.fill: parent
        onClicked: root.clicks += 1
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

#[test]
fn a_window_takes_posted_input_and_a_watcher_sees_who_gave_it() {
    let engine = ljos::engine();
    let mut window = TestWindow::new(&engine, 100.0, 100.0);
    let root = load(&engine);
    window.show(root).unwrap();
    let area = engine.children(root)[0];

    let seen = Rc::new(RefCell::new(Vec::new()));
    let into = Rc::clone(&seen);
    watch_input(&engine, area, move |_, got| {
        if let Input::Pointer(p) = got.input {
            into.borrow_mut().push((p.kind, got.origin));
        }
    });

    // The test window drives the scene as an agent's inspector does.
    let t = window.now();
    window.pointer(&PointerEvent::press(50.0, 50.0, t));
    window.pointer(&PointerEvent::release(50.0, 50.0, t));
    // The application posts; the window's next frame hands it on.
    post_input(
        &engine,
        root,
        Input::Pointer(PointerEvent::press(50.0, 50.0, t)),
    )
    .unwrap();
    post_input(
        &engine,
        root,
        Input::Pointer(PointerEvent::release(50.0, 50.0, t)),
    )
    .unwrap();
    window.settle().unwrap();

    assert_eq!(engine.read_by_name(root, "clicks").to_number(), 2.0);
    let seen = seen.borrow();
    assert!(!seen.is_empty());
    assert!(
        seen.iter().all(|(_, o)| *o == InputOrigin::Synthetic),
        "{seen:?}"
    );
}
