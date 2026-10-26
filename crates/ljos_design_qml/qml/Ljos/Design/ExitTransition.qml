// Written by ljos_design (crates/ljos_design/src/module.rs); `ljos design export` writes it again.
import QtQuick
import QtQuick.Controls

// How a popup, a dialog or a menu goes: it fades out and shrinks to 95% of its size,
// from the side its anchor is on. With reduced motion it only fades.
Transition {
    id: root
    // The point it scales about: Popup.Top for a menu below its button.
    property int origin: Popup.Center
    property int duration: 200
    readonly property bool reduced: Qt.styleHints.accessibility !== undefined && Qt.styleHints.accessibility.motionPreference === 1

    PropertyAction { property: "transformOrigin"; value: root.origin }
    NumberAnimation { property: "opacity"; from: 1; to: 0; duration: root.reduced ? Math.min(root.duration, 200) : root.duration; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.165, 0.84, 0.44, 1, 1, 1] }
    NumberAnimation { property: "scale"; from: 1; to: root.reduced ? 1 : 0.95; duration: root.reduced ? 0 : root.duration; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.165, 0.84, 0.44, 1, 1, 1] }
}
