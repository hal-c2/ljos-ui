import QtQuick
import QtQuick.Controls

Button {
    id: button
    text: qsTr("Pulse")
    onClicked: pulse.start()
    SequentialAnimation {
        id: pulse
        NumberAnimation { target: button; property: "scale"; to: 1.1; duration: 120; easing.type: Easing.OutCubic }
        NumberAnimation { target: button; property: "scale"; to: 1; duration: 120; easing.type: Easing.OutCubic }
    }
}
