// expect: color-text-contrast at 12
// audit: scheme=light
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 360; height: 120
    Rectangle {
        anchors.centerIn: parent; width: 300; height: 48
        color: "#33ff9800"
        Text { anchors.centerIn: parent; text: "Storage is almost full"; color: "#ff9800" }
    }
}
