// expect: color-semantic-hue at 9
import QtQuick
import QtQuick.Controls

Rectangle {
    width: 360; height: 240
    color: palette.window
    // The destructive action is drawn in the colour of success.
    Button {
        x: 40; y: 40
        text: "Delete"
        palette.button: "#26a269"
        palette.buttonText: "white"
    }
}
