// expect: layout-offscreen at 7
import QtQuick
import QtQuick.Controls
Rectangle {
    width: 640; height: 480
    color: "white"
    Button {
        x: 600; y: 8
        text: "Continue"
    }
}
