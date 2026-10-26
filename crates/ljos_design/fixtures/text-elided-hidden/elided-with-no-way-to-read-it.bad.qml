// expect: text-elided-hidden at 7
import QtQuick
import QtQuick.Controls
Rectangle {
    width: 640; height: 480
    color: "white"
    Text {
        x: 8; y: 8; width: 60
        text: "A sentence too long for its box"
        elide: Text.ElideRight
        color: "black"
    }
}
