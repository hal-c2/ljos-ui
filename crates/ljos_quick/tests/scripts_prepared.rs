//! A host prepares the scripts a window may run on input once the first
//! frame is shown, so that the first click runs its handler and not the
//! start of the JS host as well (about 3 ms in a release build).

mod common;

use common::*;
use ljos_quick::PointerEvent;
use ljos_runtime::Time;

const HANDLER: &str = r#"
import QtQuick
Rectangle {
    id: rect
    width: 100; height: 100
    property int n: 0
    function say(text) { n++ }
    MouseArea { anchors.fill: parent; onClicked: rect.say("clicked") }
}
"#;

const BINDINGS: &str = r#"
import QtQuick
Rectangle {
    width: 100; height: 100
    Rectangle { anchors.fill: parent; color: "red" }
}
"#;

#[test]
fn a_file_with_a_handler_has_its_script_ready_before_the_first_click() {
    let e = engine();
    let root = load(&e, HANDLER);
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);
    assert!(!e.js_started(), "nothing at load needed JS");

    e.prepare_scripts();
    assert!(e.js_started());

    s.pointer_event(&e, &PointerEvent::press(50.0, 50.0, Time::ZERO));
    s.pointer_event(
        &e,
        &PointerEvent::release(50.0, 50.0, Time::from_millis(10)),
    );
    assert_eq!(num(&e, root, "n"), 1.0);
}

#[test]
fn a_file_of_bindings_alone_starts_no_script() {
    let e = engine();
    let root = load(&e, BINDINGS);
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);
    e.prepare_scripts();
    assert!(!e.js_started());
}
