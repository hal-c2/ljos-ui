// expect: color-literal at 11
import QtQuick

Rectangle {
    width: 200
    height: 40
    color: palette.window
    Rectangle {
        anchors.fill: parent
        anchors.margins: 4
        border.color: "#3366cc"
        color: "transparent"
    }
}
