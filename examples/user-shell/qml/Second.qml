import QtQuick
import QtQuick.Controls
import Ljos.Blocks

// The window `--second` opens beside the first: the same shell.
ApplicationWindow {
    objectName: "second-window"
    width: 360
    height: 240
    visible: true
    title: "User shell, second window"

    ShellLoader {
        objectName: "second-shell"
        anchors.fill: parent
        anchors.margins: 16
        source: "Shell.qml"
        fallback: "DefaultShell.qml"
    }
}
