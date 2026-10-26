import QtQuick
import QtQuick.Controls
Rectangle {
    width: 640; height: 480
    color: "white"
    Rectangle {
        x: 8; y: 8; width: 24; height: 24
        color: "#c62828"
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: "Close"
    }
}
