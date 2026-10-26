// audit: scheme=light reduced-motion=false
import QtQuick
import QtQuick.Controls

Pane {
    id: page
    width: 300; height: 200
    background: Rectangle {
        color: page.palette.window
    }
    Label { text: qsTr("Inbox") }
}
