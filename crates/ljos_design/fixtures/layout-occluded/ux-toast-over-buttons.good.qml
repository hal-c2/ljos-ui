import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 300
    Row {
        id: actions
        anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.margins: 16
        spacing: 8
        Button { text: "Cancel" }
        Button { text: "Save" }
    }
    Rectangle {
        anchors.right: parent.right; anchors.bottom: actions.top; anchors.margins: 16
        width: 200; height: 40; z: 2
        color: palette.toolTipBase
        Label { anchors.centerIn: parent; text: "Saved"; color: palette.toolTipText }
    }
}
