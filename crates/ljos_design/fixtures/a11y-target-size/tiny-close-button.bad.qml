// expect: a11y-target-size at 7
import QtQuick
import QtQuick.Controls.Basic
Rectangle {
    width: 640; height: 480
    color: "white"
    Rectangle {
        x: 8; y: 8; width: 16; height: 16
        color: "#c62828"
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: "Close"
    }
}
