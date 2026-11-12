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
        buttons: MouseButton.Right; modifiers: Modifier.Control\n\
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
        // A `var` is its function's, wherever in it it is declared.
        "import QtQuick\nRectangle { function f() { if (true) { var fill = 1 } return fill } }\n",
        // A loop's and a `catch`'s names are theirs.
        "import QtQuick\nRectangle { function f() { let t = 0; for (const fill of [1]) t += fill; return t } }\n",
        "import QtQuick\nRectangle { function f() { for (let fill = 0; fill < 2; fill++) {} return 0 } }\n",
        "import QtQuick\nRectangle { function f() { try { return 1 } catch (fill) { return fill } } }\n",
    ] {
        lowered(source);
    }
    let capture = "`fill` would be the `Rectangle`'s own once lowered (Qt's `color`), not what it names in Qt";
    for source in [
        // A `let` declared in an inner block, read outside it.
        "import QtQuick\nRectangle { function f() { if (true) { let fill = 1 } return fill } }\n",
        // A loop's name read after the loop.
        "import QtQuick\nRectangle { function f() { for (const fill of [1]) {} return fill } }\n",
        // A parameter of another function.
        "import QtQuick\nRectangle { function f(fill) { return 1 }\n    function g() { return fill } }\n",
        "import QtQuick\nRectangle { function f() { const g = (fill) => fill; return fill } }\n",
    ] {
        assert_eq!(kept_for(source), capture, "{source}");
    }
}

/// A parameter or a local hides an id of the file, in JS as once lowered:
/// what it holds is not known, so a member Ljós renames read off it keeps
/// the file rather than being renamed as the id's.
#[test]
fn a_name_a_script_binds_hides_an_id() {
    for (source, member) in [
        (
            "import QtQuick\nRectangle { id: box; function tint(box) { return box.color } }\n",
            "color",
        ),
        (
            "import QtQuick\nItem { TapHandler { id: tap }\n    function f(tap) { return tap.acceptedButtons } }\n",
            "acceptedButtons",
        ),
        (
            "import QtQuick\nRectangle { id: box; opacity: { const box = other; return box.color.a } }\n",
            "color",
        ),
        // A loop's, a `catch`'s or an inner block's `var` hides it too.
        (
            "import QtQuick\nRectangle { id: box; function f() { for (const box of others) box.color = 1 } }\n",
            "color",
        ),
        (
            "import QtQuick\nRectangle { id: box; function f() { try {} catch (box) { return box.color } } }\n",
            "color",
        ),
        (
            "import QtQuick\nRectangle { id: box; function f() { if (true) { var box = other } return box.color } }\n",
            "color",
        ),
    ] {
        assert_eq!(
            kept_for(source),
            format!("`.{member}` is read off something whose type is not known"),
            "{source}"
        );
    }
    // Where nothing binds the name, it is the id.
    assert_eq!(
        lowered("import QtQuick\nRectangle { id: box; function tint(c) { return box.color } }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui\n\
         Rectangle { id: box; function tint(c) { return box.fill } }\n"
    );
}

/// A name a script binds hides a type of the file's and a member of the
/// object around the script, as it hides an id: what is read off it is
/// not the type's or the member's.
#[test]
fn a_name_a_script_binds_hides_a_type_and_a_member() {
    for source in [
        "import QtQuick\nItem { component Swatch: Rectangle {}\n    function f(list) { for (const Swatch of list) return Swatch.color } }\n",
        "import QtQuick\nItem { component Swatch: Rectangle {}\n    function f() { try {} catch (Swatch) { return Swatch.color } } }\n",
    ] {
        assert_eq!(
            kept_for(source),
            "`.color` is read off something whose type is not known",
            "{source}"
        );
    }
    // What a local holds is not followed as a font (docs/DIALECT.md §4.13):
    // the loop's `font` is not the Text's, so its `pixelSize` stays.
    let source = "import QtQuick\nText { function f(list) { for (const font of list) return font.pixelSize } }\n";
    assert!(
        lowered(source).contains("return font.pixelSize"),
        "{source}"
    );
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

#[test]
fn the_drag_point_and_wheel_handlers_are_lowered_to_gestures() {
    let source = "import QtQuick\n\
Item {\n\
    DragHandler { acceptedButtons: Qt.RightButton; snapMode: DragHandler.NoSnap }\n\
    PointHandler { id: point; acceptedDevices: PointerDevice.Stylus }\n\
    WheelHandler { acceptedModifiers: Qt.ControlModifier }\n\
    property bool pen: point.active && point.acceptedDevices > 0\n\
}\n";
    assert_eq!(
        lowered(source),
        "pragma Scripts: JavaScript; import Ljos.Ui\n\
Item {\n\
    Drag { buttons: MouseButton.Right; snapMode: Drag.NoSnap }\n\
    Pointer { id: point; devices: PointerDevice.Stylus }\n\
    Wheel { modifiers: Modifier.Control }\n\
    property bool pen: point.active && point.devices > 0\n\
}\n"
    );
}

/// Qt's attached `Drag` is `DragSource`, set on the item, read in a script
/// and read off another item; its enums go with it.
#[test]
fn the_attached_drag_is_a_drag_source() {
    let source = "import QtQuick\n\
Item {\n\
    id: card\n\
    Drag.active: true; Drag.dragType: Drag.Automatic\n\
    Drag.onDragStarted: console.log(Drag.active, card.Drag.source)\n\
    MouseArea { drag.target: parent; drag.axis: Drag.XAxis }\n\
}\n";
    assert_eq!(
        lowered(source),
        "pragma Scripts: JavaScript; import Ljos.Ui\n\
Item {\n\
    id: card\n\
    DragSource.active: true; DragSource.dragType: DragSource.Automatic\n\
    DragSource.onDragStarted: console.log(DragSource.active, card.DragSource.source)\n\
    PointerArea { drag.target: parent; drag.axis: DragSource.XAxis }\n\
}\n"
    );
}

/// A `MouseArea` that fills its parent and only takes clicks is a `Tap` on
/// the parent: one that taps when let go inside, however the pointer moved
/// and however long it was held, as a click is.
#[test]
fn a_mouse_area_filling_its_parent_is_a_tap() {
    let source = "import QtQuick\n\
Rectangle {\n\
    id: card\n\
    MouseArea { anchors.fill: parent; onClicked: card.color = \"red\" }\n\
}\n";
    assert_eq!(
        lowered(source),
        "pragma Scripts: JavaScript; import Ljos.Ui\n\
Rectangle {\n\
    id: card\n\
    Tap { gesturePolicy: Tap.ReleaseWithinBounds; longPressThreshold: 0; onTapped: card.fill = \"red\" }\n\
}\n"
    );
}

/// The cursor it asks for is a `Hover`'s, after the `Tap` on its closing
/// line, disabled as the area is; its buttons are the `Tap`'s.
#[test]
fn a_filling_mouse_area_s_cursor_is_a_hover_s() {
    let source = "import QtQuick\n\
Item {\n\
    id: root\n\
    property bool on: true\n\
    Rectangle {\n\
        id: card\n\
        MouseArea {\n\
            id: area; objectName: \"area\"\n\
            anchors.fill: parent\n\
            enabled: root.on && card.color != \"red\"\n\
            cursorShape: Qt.PointingHandCursor\n\
            acceptedButtons: Qt.RightButton\n\
            onClicked: {\n\
                card.color = \"red\"\n\
            }\n\
        }\n\
    }\n\
}\n";
    assert_eq!(
        lowered(source),
        "pragma Scripts: JavaScript; import Ljos.Ui\n\
Item {\n\
    id: root\n\
    property bool on: true\n\
    Rectangle {\n\
        id: card\n\
        Tap {\n\
            id: area; objectName: \"area\"\n\
            gesturePolicy: Tap.ReleaseWithinBounds; longPressThreshold: 0\n\
            enabled: root.on && card.fill != \"red\"\n\
\n\
            buttons: MouseButton.Right\n\
            onTapped: {\n\
                card.fill = \"red\"\n\
            }\n\
        } Hover { enabled: root.on && card.fill != \"red\"; cursorShape: Cursor.PointingHand }\n\
    }\n\
}\n"
    );
}

/// A `MouseArea` that would not act as a `Tap` on its parent stays a
/// `PointerArea`, and its file is lowered all the same.
#[test]
fn a_mouse_area_a_tap_would_not_be_stays_a_pointer_area() {
    let area = "MouseArea { anchors.fill: parent; onClicked: console.log(1) }";
    let cases = [
        // Something that takes the pointer is under it, and would be over a Tap.
        // (A type of the project's may hold anything.)
        format!("Item {{ Card {{}} {area} }}"),
        format!("Item {{ Item {{ MouseArea {{}} }} {area} }}"),
        format!("Item {{ Item {{ TapHandler {{}} }} {area} }}"),
        // A handler beside it is on the same item as the Tap would be.
        format!("Item {{ {area} HoverHandler {{}} }}"),
        // It, or something beside it, is stacked out of order.
        format!("Item {{ Rectangle {{ z: 2 }} {area} }}"),
        "Item { MouseArea { anchors.fill: parent; z: 1; onClicked: console.log(1) } }".into(),
        // Its parent is not an item that only draws, or answers for where it is.
        format!("Item {{ Text {{ {area} }} }}"),
        format!("Item {{ Card {{ {area} }} }}"),
        format!("Item {{ containmentMask: mask; Item {{ id: mask }} {area} }}"),
        // It does not fill its parent, or does more than fill it.
        "Item { MouseArea { anchors.fill: sibling; onClicked: console.log(1) } Item { id: sibling } }"
            .into(),
        "Item { MouseArea { anchors.fill: parent; anchors.margins: 4 } }".into(),
        "Item { MouseArea { anchors.fill: parent; Item {} } }".into(),
        // It does what a Tap or a Hover does not, or not as they do it.
        "Item { MouseArea { anchors.fill: parent; onPressed: console.log(1) } }".into(),
        "Item { MouseArea { anchors.fill: parent; hoverEnabled: true } }".into(),
        "Item { MouseArea { anchors.fill: parent; drag.target: parent } }".into(),
        "Item { MouseArea { anchors.fill: parent; property int n: 0 } }".into(),
        // Its handler reads the click, or a name a Tap or the area has.
        "Item { MouseArea { anchors.fill: parent; onClicked: console.log(mouse.x) } }".into(),
        "Item { MouseArea { anchors.fill: parent; onClicked: (m) => console.log(m.x) } }".into(),
        "Item { MouseArea { anchors.fill: parent; onClicked: console.log(pressed) } }".into(),
        "Item { MouseArea { anchors.fill: parent; onClicked: console.log(width) } }".into(),
        "Item { property int point: 0; MouseArea { anchors.fill: parent; onClicked: point += 1 } }"
            .into(),
        // It is read by its id.
        "Item { MouseArea { id: area; anchors.fill: parent } property bool down: area.pressed }"
            .into(),
        // What the Hover would copy spans lines.
        "Item { MouseArea { anchors.fill: parent\n cursorShape: true\n ? Qt.PointingHandCursor : Qt.ArrowCursor } }"
            .into(),
        // It is the root.
        "MouseArea { anchors.fill: parent }".into(),
    ];
    for case in cases {
        let source = format!("import QtQuick\n{case}\n");
        let Outcome::Lowered(Lowered { text, .. }) = lower(&source, &no_locals) else {
            panic!("{case}: not lowered");
        };
        assert!(
            text.contains("PointerArea {") && !text.contains("ReleaseWithinBounds"),
            "{case}\nbecame\n{text}"
        );
    }
}
/// Only an item has Qt's attached `Drag`. A key of that name on anything
/// else is the object's own, and is not renamed.
#[test]
fn a_drag_read_off_what_is_no_item_is_not_the_attached_drag() {
    // A JavaScript object's key: what the object is is not known.
    let source = "import QtQuick\n\
Item {\n\
    function speed() { var config = { Drag: { speed: 2 } }; return config.Drag.speed }\n\
}\n";
    assert_eq!(
        kept_for(source),
        "`.Drag` is read off something whose type is not known, and only an item's is Qt's attached `Drag`"
    );
    // An object that is no item.
    assert_eq!(
        kept_for("import QtQuick\nItem { Timer { id: tick }\n    property var d: tick.Drag }\n"),
        "`.Drag` is read off a `Timer`, which is no item, and only an item's is Qt's attached `Drag`"
    );
    // A value has no attached objects: the key stays as written.
    assert_eq!(
        lowered("import QtQuick\nItem { property var d: \"x\".Drag }\n"),
        "pragma Scripts: JavaScript; import Ljos.Ui\nItem { property unknown d: \"x\".Drag }\n"
    );
    // An item, named by its id or as `parent`, has it.
    assert_eq!(
        lowered(
            "import QtQuick\nItem { id: card\n    Item { property bool a: card.Drag.active || parent.Drag.active }\n}\n"
        ),
        "pragma Scripts: JavaScript; import Ljos.Ui\nItem { id: card\n    Item { property bool a: card.DragSource.active || parent.DragSource.active }\n}\n"
    );
}
