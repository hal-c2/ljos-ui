// expect: layout-popup-fit at 7
import QtQuick
import QtQuick.Controls
Rectangle {
    width: 640; height: 480
    color: "white"
    Popup {
        x: 500; y: 100; width: 300; height: 120
        visible: true
        Label { text: "Saved" }
    }
}
