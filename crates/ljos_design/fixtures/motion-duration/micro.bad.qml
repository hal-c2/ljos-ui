// expect: motion-duration at 12
import QtQuick
import QtQuick.Controls

Button {
    id: button
    text: qsTr("Save")
    background: Rectangle {
        opacity: button.hovered ? 1 : 0.8
        Behavior on opacity {
            NumberAnimation {
                duration: 400
                easing.type: Easing.OutCubic
            }
        }
    }
}
