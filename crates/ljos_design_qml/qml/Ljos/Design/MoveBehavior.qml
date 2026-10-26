// Written by ljos_design (crates/ljos_design/src/module.rs); `ljos design export` writes it again.
import QtQuick

// Something on screen that moves to another place: `MoveBehavior on x {}`.
// With reduced motion it is there at once.
Behavior {
    id: root
    property int duration: 250
    readonly property bool reduced: Qt.styleHints.accessibility !== undefined && Qt.styleHints.accessibility.motionPreference === 1
    enabled: !reduced

    NumberAnimation { duration: root.duration; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.77, 0, 0.175, 1, 1, 1] }
}
