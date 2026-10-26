import QtQuick

Rectangle {
    width: 200
    height: 80
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: qsTr("Open the report")
    Accessible.onPressAction: console.log("open")
    Keys.onReturnPressed: console.log("open")
    MouseArea {
        anchors.fill: parent
        onClicked: console.log("open")
    }
}
