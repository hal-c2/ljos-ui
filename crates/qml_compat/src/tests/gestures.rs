//! Tests of gestures and handlers: `TapHandler`, `DragHandler`, flicking.

use super::*;

#[test]
fn handlers_are_lowered_to_gestures_by_member() {
    let source = "import QtQuick\n\
Rectangle {\n\
    color: hover.hovered ? \"red\" : \"blue\"\n\
    HoverHandler { id: hover; acceptedDevices: PointerDevice.Mouse; onAcceptedDevicesChanged: {} }\n\
    TapHandler {\n\
        id: tap\n\
        acceptedButtons: Qt.RightButton; acceptedModifiers: Qt.ControlModifier\n\
        gesturePolicy: TapHandler.WithinBounds\n\
        onTapped: console.log(tap.acceptedButtons, hover.acceptedPointerTypes)\n\
    }\n\
}\n";
    assert_eq!(
        lowered(source),
        "pragma Scripts: JavaScript; import Ljos.Ui\n\
Rectangle {\n\
    fill: hover.hovered ? \"red\" : \"blue\"\n\
    Hover { id: hover; devices: PointerDevice.Mouse; onDevicesChanged: {} }\n\
    Tap {\n\
        id: tap\n\
        buttons: Qt.RightButton; modifiers: Qt.ControlModifier\n\
        gesturePolicy: Tap.WithinBounds\n\
        onTapped: console.log(tap.buttons, hover.pointerTypes)\n\
    }\n\
}\n"
    );
}

#[test]
fn a_member_a_gesture_renames_read_off_any_object_keeps_the_file() {
    // A QtObject may be a handler, as an Item may be a Rectangle.
    let source = "import QtQuick\n\
Item {\n\
    property QtObject handler\n\
    property int buttons: handler.acceptedButtons\n\
}\n";
    assert_eq!(
        kept_for(source),
        "`.acceptedButtons` is read off something whose type is not known"
    );
    // A Timer may not be one.
    assert!(matches!(
        lower(
            "import QtQuick\nItem { Timer { id: t }\n    property var b: t.acceptedButtons }\n",
            &no_locals
        ),
        Outcome::Lowered(_)
    ));
    // What an Item may be does not rename what only a handler does.
    assert!(matches!(
        lower(
            "import QtQuick\nItem { property int b: parent.acceptedButtons }\n",
            &no_locals
        ),
        Outcome::Lowered(_)
    ));
}
