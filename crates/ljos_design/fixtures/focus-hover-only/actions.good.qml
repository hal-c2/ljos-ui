import QtQuick
import QtQuick.Controls

Rectangle {
    id: row
    width: 300
    height: 40
    activeFocusOnTab: true
    HoverHandler {
        id: hover
    }
    Row {
        anchors.right: parent.right
        visible: hover.hovered || row.activeFocus
        Button {
            text: qsTr("Delete")
        }
    }
}
