// expect: layout-window-controls at 14
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    width: 600; height: 300; visible: true
    flags: Qt.Window | Qt.FramelessWindowHint
    RowLayout {
        anchors.fill: parent; spacing: 0
        ColumnLayout {
            Layout.fillWidth: true; Layout.fillHeight: true
            Row {
                ToolButton { text: "Min" }
                ToolButton { text: "Close"; onClicked: close() }
            }
            Item { Layout.fillHeight: true }
        }
        Rectangle { Layout.preferredWidth: 240; Layout.fillHeight: true; color: palette.window }
    }
}
