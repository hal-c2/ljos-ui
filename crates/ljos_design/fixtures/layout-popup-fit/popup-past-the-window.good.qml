import QtQuick
import QtQuick.Controls.Basic
Rectangle {
    width: 640; height: 480
    color: "white"
    Popup {
        x: 170; y: 100
        visible: true
        Label { text: "Saved" }
    }
}
