import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    width: 600; height: 300; visible: true
    flags: Qt.Window | Qt.FramelessWindowHint
    header: Item {
        implicitHeight: 36
        Row {
            anchors.right: parent.right; anchors.top: parent.top
            ToolButton { text: "Min"; onClicked: showMinimized() }
            ToolButton { text: "Close"; onClicked: close() }
        }
    }
    RowLayout {
        anchors.fill: parent; spacing: 0
        Item { Layout.fillWidth: true; Layout.fillHeight: true }
        Rectangle { Layout.preferredWidth: 240; Layout.fillHeight: true; color: palette.window }
    }
}
