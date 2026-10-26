import QtQuick
import QtQuick.Controls

Rectangle {
    width: 300
    height: 40
    HoverHandler {
        id: hover
    }
    Button {
        anchors.right: parent.right
        text: qsTr("Delete")
        opacity: hover.hovered ? 1 : 0.6
    }
}
