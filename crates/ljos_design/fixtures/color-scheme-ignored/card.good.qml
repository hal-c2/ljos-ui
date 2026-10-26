import QtQuick

Rectangle {
    width: 200
    height: 80
    color: palette.base
    Text {
        anchors.centerIn: parent
        text: qsTr("Card")
        color: palette.text
    }
}
