// expect: focus-pointer-only at 11
import QtQuick

Rectangle {
    width: 200
    height: 80
    Accessible.role: Accessible.Button
    Accessible.name: qsTr("Open the report")
    MouseArea {
        anchors.fill: parent
        onClicked: console.log("open")
    }
}
