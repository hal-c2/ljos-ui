import QtQuick

Rectangle {
    width: 120
    height: 40
    activeFocusOnTab: true
    Keys.onReturnPressed: console.log("open")
    Accessible.role: Accessible.Button
    Accessible.name: qsTr("Open")
    MouseArea {
        anchors.fill: parent
        onClicked: console.log("open")
    }
}
