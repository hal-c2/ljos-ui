// Written by ljos_design (crates/ljos_design/src/module.rs); `ljos design export` writes it again.
import QtQuick

// A control giving under a press: `PressBehavior on scale {}` with
// `scale: pressed ? Motion.pressScale : 1`. With reduced motion it is
// there at once.
Behavior {
    id: root
    property int duration: 100
    readonly property bool reduced: Qt.styleHints.accessibility !== undefined && Qt.styleHints.accessibility.motionPreference === 1
    enabled: !reduced

    NumberAnimation { duration: root.duration; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.23, 1, 0.32, 1, 1, 1] }
}
