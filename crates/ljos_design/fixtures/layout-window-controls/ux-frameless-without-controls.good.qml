import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    width: 400; height: 300; visible: true
    flags: Qt.Window | Qt.FramelessWindowHint
    header: Item {
        implicitHeight: 36
        Label { anchors.verticalCenter: parent.verticalCenter; x: 12; text: "Title" }
        Row {
            anchors.right: parent.right; anchors.top: parent.top
            ToolButton { text: "Min"; onClicked: showMinimized() }
            ToolButton { text: "Max"; onClicked: showMaximized() }
            ToolButton { text: "Close"; onClicked: close() }
        }
    }
    Label { anchors.centerIn: parent; text: "Content" }
}
