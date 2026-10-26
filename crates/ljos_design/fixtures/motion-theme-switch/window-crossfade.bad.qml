// expect: motion-theme-switch at 12
// audit: scheme=light reduced-motion=false
import QtQuick
import QtQuick.Controls

Pane {
    id: page
    width: 300; height: 200
    background: Rectangle {
        color: page.palette.window
        Behavior on color {
            ColorAnimation { duration: 600 }
        }
    }
    Label { text: qsTr("Inbox") }
}
