import QtQuick
import QtQuick.Controls

Button {
    id: button
    text: qsTr("Shake")
    onClicked: shake.start()
    NumberAnimation { id: shake; target: button; property: "rotation"; from: -3; to: 0; duration: 150; easing.type: Easing.OutCubic }
}
