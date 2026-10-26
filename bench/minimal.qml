// The least a window costs: one Window with one Text, nothing moving.
import QtQuick

Window {
    width: 400
    height: 300
    visible: true
    color: "white"

    Text {
        anchors.centerIn: parent
        text: "Hello"
        font.pixelSize: 24
    }
}
