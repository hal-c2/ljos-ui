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

/// A name a renamed member is called now would be found on the object
/// where, in Qt, it is found further out: the file is kept.
#[test]
fn a_new_name_that_would_capture_an_outer_one_keeps_the_file() {
    let source = "import QtQuick\n\
Item {\n\
    property int buttons: 0\n\
    TapHandler { onTapped: buttons += 1 }\n\
}\n";
    assert_eq!(
        kept_for(source),
        "`buttons` would be the `Tap`'s own once lowered (Qt's `acceptedButtons`), not what it names in Qt"
    );
    let source = "import QtQuick\n\
Item {\n\
    property color fill: \"red\"\n\
    Rectangle { color: fill }\n\
}\n";
    assert_eq!(
        kept_for(source),
        "`fill` would be the `Rectangle`'s own once lowered (Qt's `color`), not what it names in Qt"
    );
    // Not the scope's, the name is looked up on the root next: in Qt it is
    // found outside the file, once lowered on the root.
    let source = "import QtQuick\n\
Rectangle {\n\
    Text { text: fill }\n\
}\n";
    assert_eq!(
        kept_for(source),
        "`fill` would be the `Rectangle`'s own once lowered (Qt's `color`), not what it names in Qt"
    );
    let source = "import QtQuick\n\
TapHandler {\n\
    property QtObject counter: QtObject { property int seen: buttons }\n\
}\n";
    assert_eq!(
        kept_for(source),
        "`buttons` would be the `Tap`'s own once lowered (Qt's `acceptedButtons`), not what it names in Qt"
    );
    // A scope that has the name finds it before the root does.
    assert!(matches!(
        lower(
            "import QtQuick\nRectangle { Item { property color fill: \"red\"; opacity: fill == \"red\" ? 1 : 0 } }\n",
            &no_locals
        ),
        Outcome::Lowered(_)
    ));
    // Read off the outer object by its id, it is still that object's.
    assert!(matches!(
        lower(
            "import QtQuick\nItem { id: root; property int buttons: 0\n    TapHandler { onTapped: root.buttons += 1 } }\n",
            &no_locals
        ),
        Outcome::Lowered(_)
    ));
}

/// A parameter or a local of a renamed member's new name is the script's,
/// in Qt as once lowered: the file is lowered. One that may not be bound
/// where it is read still keeps it.
#[test]
fn a_new_name_a_script_binds_is_its_own() {
    for source in [
        "import QtQuick\nRectangle { function shade(fill) { return Qt.darker(fill) } }\n",
        "import QtQuick\nRectangle { function shade(...fill) { return fill.length } }\n",
        "import QtQuick\nRectangle { function shade({ fill }) { return fill } }\n",
        "import QtQuick\nItem { TapHandler { onTapped: (buttons) => console.log(buttons) } }\n",
        "import QtQuick\nRectangle { opacity: { const fill = 1; return fill } }\n",
        "import QtQuick\nRectangle { function f() { if (true) { return fill } var fill = 1 } }\n",
        "import QtQuick\nRectangle { function f() { function fill() { return 1 } return fill() } }\n",
    ] {
        lowered(source);
    }
    let capture = "`fill` would be the `Rectangle`'s own once lowered (Qt's `color`), not what it names in Qt";
    for source in [
        // Declared in an inner block, read outside it.
        "import QtQuick\nRectangle { function f() { if (true) { var fill = 1 } return fill } }\n",
        // A parameter of another function.
        "import QtQuick\nRectangle { function f(fill) { return 1 }\n    function g() { return fill } }\n",
        "import QtQuick\nRectangle { function f() { const g = (fill) => fill; return fill } }\n",
    ] {
        assert_eq!(kept_for(source), capture, "{source}");
    }
}

/// Qt's grab permissions are a gesture's `takeOver`, which is not built
/// (docs/DIALECT.md §4.7): set, read or named by their keys, they keep the
/// file.
#[test]
fn grab_permissions_keep_the_file() {
    let reason =
        "`grabPermissions` is a gesture's `takeOver` in Ljós's API, which is not built yet";
    for source in [
        "import QtQuick\nItem { TapHandler { grabPermissions: 15 } }\n",
        "import QtQuick\nItem { TapHandler { grabPermissions: TapHandler.CanTakeOverFromAnything } }\n",
        "import QtQuick\nItem { HoverHandler { onGrabPermissionsChanged: {} } }\n",
        "import QtQuick\nItem { TapHandler { id: tap }\n    property int g: tap.grabPermissions }\n",
        "import QtQuick\nItem { TapHandler { onTapped: console.log(grabPermissions) } }\n",
        "import QtQuick\nItem { property int g: HoverHandler.ApprovesTakeOverByAnything }\n",
    ] {
        assert_eq!(kept_for(source), reason, "{source}");
    }
}
