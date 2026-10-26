// expect: text-clipped at 11
import QtQuick
import QtQuick.Controls
Rectangle {
    width: 640; height: 480
    color: "white"
    Rectangle {
        x: 8; y: 8; width: 60; height: 24
        clip: true
        color: "#eeeeee"
        Text {
            text: "A sentence too long for its box"
            color: "black"
        }
    }
}
