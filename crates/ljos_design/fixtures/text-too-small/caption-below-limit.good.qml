import QtQuick
import QtQuick.Controls

Item {
    width: 300; height: 120
    Column {
        anchors.centerIn: parent
        Label { text: "Quarterly report"; font.bold: true }
        Label { text: "Updated an hour ago"; font.pixelSize: 12; opacity: 0.8 }
    }
}
