// Written by ljos_design (crates/ljos_design/src/module.rs); `ljos design export` writes it again.
import QtQuick
import QtQuick.Controls

// How a popup, a dialog or a menu appears: it fades in and grows from 95% of its size,
// from the side its anchor is on. With reduced motion it only fades.
Transition {
    id: root
    // The point it scales about: Popup.Top for a menu below its button.
    property int origin: Popup.Center
    property int duration: 250
    readonly property bool reduced: Qt.styleHints.accessibility !== undefined && Qt.styleHints.accessibility.motionPreference === 1

    PropertyAction { property: "transformOrigin"; value: root.origin }
    NumberAnimation { property: "opacity"; from: 0; to: 1; duration: root.reduced ? Math.min(root.duration, 200) : root.duration; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.23, 1, 0.32, 1, 1, 1] }
    NumberAnimation { property: "scale"; from: root.reduced ? 1 : 0.95; to: 1; duration: root.reduced ? 0 : root.duration; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.23, 1, 0.32, 1, 1, 1] }
}
