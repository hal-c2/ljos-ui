// audit: scheme=light reduced-motion=false
import QtQuick
import QtQuick.Controls

Pane {
    id: page
    width: 300; height: 200
    Rectangle {
        width: 260; height: 60
        color: page.palette.button
        Behavior on color {
            ColorAnimation { duration: 80 }
        }
    }
}
