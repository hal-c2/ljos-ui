// expect: color-text-contrast at 7
import QtQuick
import QtQuick.Controls.Basic
Rectangle {
    width: 640; height: 480
    color: "white"
    Text {
        x: 8; y: 8
        text: "Last saved an hour ago"
        color: "#bbbbbb"
    }
}
