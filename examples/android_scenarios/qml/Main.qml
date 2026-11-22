import QtQuick

// What shows until a scenario's own Main.qml is laid over this one.
Window {
    objectName: "waiting"
    visible: true
    color: "#10141c"

    Text {
        anchors.centerIn: parent
        color: "#9aa4b8"
        font.pixelSize: 18
        text: "Waiting for a scenario"
    }
}
