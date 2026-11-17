import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Ljos.Blocks

ApplicationWindow {
    objectName: "window"
    width: 480
    height: 320
    visible: true
    title: "User shell"

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        TextField {
            objectName: "note"
            Layout.fillWidth: true
            placeholderText: "Type here, then save a Shell.qml"
            focus: true
        }

        ShellLoader {
            objectName: "shell"
            Layout.fillWidth: true
            Layout.fillHeight: true
            source: "Shell.qml"
            fallback: "DefaultShell.qml"
        }
    }
}
