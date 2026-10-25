import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 360; height: 120
    Rectangle {
        anchors.centerIn: parent; width: 300; height: 48
        color: Qt.alpha(palette.highlight, 0.15)
        Text { anchors.centerIn: parent; text: "Storage is almost full"; color: palette.windowText }
    }
}
