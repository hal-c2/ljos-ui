import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 100
    Rectangle {
        width: 280; height: 40; color: palette.base
        Label { anchors.verticalCenter: parent.verticalCenter; x: 8; text: "Draft notes" }
        Button { anchors.right: parent.right; text: "Delete" }
    }
}
