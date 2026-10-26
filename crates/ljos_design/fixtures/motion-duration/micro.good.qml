import QtQuick
import QtQuick.Controls

Button {
    id: button
    text: qsTr("Save")
    background: Rectangle {
        opacity: button.hovered ? 1 : 0.8
        Behavior on opacity {
            NumberAnimation {
                duration: 150
                easing.type: Easing.OutCubic
            }
        }
    }
}
