// expect: layout-content-overflow at 8
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
Rectangle {
    width: 640; height: 480
    color: "white"
    ItemDelegate {
        x: 10; y: 10; width: 300; height: 28
        background: Rectangle { color: "#ddddee" }
        contentItem: RowLayout {
            Label { text: qsTr("Medium"); Layout.fillWidth: true }
        }
    }
}
