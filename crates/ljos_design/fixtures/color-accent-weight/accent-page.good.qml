import QtQuick
import QtQuick.Controls

Rectangle {
    width: 360; height: 240
    color: palette.window
    Rectangle {
        x: 0; y: 0
        width: parent.width; height: 4
        color: palette.highlight
    }
    Label {
        anchors.centerIn: parent
        text: "Welcome"
    }
}
