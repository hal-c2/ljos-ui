// expect: color-surface-merge at 13
import QtQuick
import QtQuick.Controls

Rectangle {
    width: 360; height: 240
    color: palette.window
    Popup {
        x: 40; y: 40
        width: 200; height: 120
        Component.onCompleted: open()
        // The popup is the colour of the page and has no edge.
        background: Rectangle {
            color: palette.window
        }
        Label { text: "Saved" }
    }
}
