import QtQuick

// Nothing here moves or runs a timer: once the first frame is drawn the
// application is idle, and only the host's asks wake it.
Window {
    id: root
    objectName: "main"
    width: 240; height: 140
    visible: true
    color: "#f4f1ea"

    function finish() { Qt.quit() }

    Rectangle {
        objectName: "card"
        x: 20; y: 20; width: 120; height: 80
        radius: 8
        color: "#2f6f8f"
        Rectangle { x: 12; y: 12; width: 40; height: 40; radius: 20; color: "#f2b134" }
        Text { x: 12; y: 56; text: "Ljós"; color: "white"; font.pixelSize: 16 }
    }
    Rectangle { objectName: "doomed"; x: 160; y: 20; width: 60; height: 30; color: "#c0392b" }
    Rectangle { objectName: "doomedNow"; x: 160; y: 70; width: 60; height: 30; color: "#8e44ad" }
}
