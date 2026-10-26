import QtQuick

Rectangle {
    width: 200
    height: 80
    Accessible.name: qsTr("Notes")
    TextInput {
        id: input
        anchors.fill: parent
    }
    MouseArea {
        anchors.fill: parent
        onClicked: input.forceActiveFocus()
    }
}
