import QtQuick

// A root window and a second one that is closed and shown again, so the
// application is handed three platform windows.
Window {
    objectName: "main"
    width: 200; height: 120
    visible: true
    color: "white"

    Window {
        id: second
        objectName: "second"
        width: 160; height: 100
        visible: true
        color: "#80000000"
        backdrop: Window.BlurBackdrop
    }

    Timer {
        interval: 400; running: true
        onTriggered: second.visible = false
    }
    Timer {
        interval: 800; running: true
        onTriggered: second.visible = true
    }
    Timer {
        interval: 1500; running: true
        onTriggered: Qt.quit()
    }
}
