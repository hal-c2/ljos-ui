import QtQuick
import QtQuick.Controls

Rectangle {
    width: 360; height: 240
    color: palette.window
    Popup {
        x: 40; y: 40
        width: 200; height: 120
        Component.onCompleted: open()
        background: Rectangle {
            color: palette.window
            border.color: palette.dark
            border.width: 1
        }
        Label { text: "Saved" }
    }
}
