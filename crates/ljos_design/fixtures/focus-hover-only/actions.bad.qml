// expect: focus-hover-only at 13
import QtQuick
import QtQuick.Controls

Rectangle {
    width: 300
    height: 40
    HoverHandler {
        id: hover
    }
    Row {
        anchors.right: parent.right
        visible: hover.hovered
        Button {
            text: qsTr("Delete")
        }
    }
}
