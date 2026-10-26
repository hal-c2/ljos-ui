import QtQuick
import QtQuick.Controls.Basic
Rectangle {
    width: 640; height: 480
    color: "white"
    Rectangle {
        x: 0; y: 0; width: 640; height: 60
        color: "#fff3e0"
    }
    Button {
        x: 8; y: 8
        text: "Send"
    }
}
