// expect: text-too-small at 11
import QtQuick
import QtQuick.Controls

Item {
    width: 300; height: 120
    Column {
        anchors.centerIn: parent
        Label { text: "Quarterly report" }
        // A caption made smaller than anyone reads comfortably.
        Label { text: "Updated an hour ago"; font.pixelSize: 8 }
    }
}
