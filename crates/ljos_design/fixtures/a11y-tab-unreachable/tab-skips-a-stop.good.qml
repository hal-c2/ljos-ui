import QtQuick
import QtQuick.Controls.Basic
Rectangle {
    width: 640; height: 480
    color: "white"
    Row {
        spacing: 8
        Button { id: a; text: "One" }
        Button { id: b; text: "Two" }
        Button { id: c; text: "Three" }
    }
}
