import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 160; height: 80
    Label {
        id: badge
        text: qsTr("New")
        SequentialAnimation on y {
            loops: Animation.Infinite
            running: Qt.styleHints.accessibility.motionPreference !== Qt.MotionPreference.ReducedMotion
            NumberAnimation { to: 4; duration: 600; easing.type: Easing.InOutSine }
            NumberAnimation { to: 0; duration: 600; easing.type: Easing.InOutSine }
        }
    }
}
