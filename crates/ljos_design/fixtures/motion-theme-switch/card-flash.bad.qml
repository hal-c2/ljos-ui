// expect: motion-theme-switch at 9
// audit: scheme=light reduced-motion=false
import QtQuick
import QtQuick.Controls

Pane {
    id: page
    width: 300; height: 200
    Rectangle {
        width: 260; height: 60
        color: page.palette.button
        // Through black on the way, from one theme's card to the other's.
        Behavior on color {
            SequentialAnimation {
                ColorAnimation { to: "black"; duration: 40 }
                ColorAnimation { duration: 40 }
            }
        }
    }
}
