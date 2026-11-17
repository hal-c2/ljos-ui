import QtQuick
import QtQuick.Controls

Rectangle {
    objectName: "default-shell"
    color: "#e8eaf0"
    radius: 8

    Label {
        anchors.centerIn: parent
        text: "The application's own shell"
    }
}
