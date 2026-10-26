import QtQuick
import QtQuick.Controls

Rectangle {
    width: 360; height: 240
    color: palette.window
    Row {
        x: 40; y: 40
        spacing: 8
        Button {
            text: "Save"
        }
        Button {
            text: "Publish"
            highlighted: true
        }
    }
}
