import QtQuick
import QtQuick.Controls
Rectangle {
    width: 640; height: 480
    color: "white"
    Rectangle {
        x: 8; y: 8; width: 40; height: 40
        color: "#1565c0"
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: "Delete"
    }
}
