// expect: motion-reduced-ignored at 10
// audit: reduced-motion=true
import QtQuick
import QtQuick.Controls

Rectangle {
    width: 300; height: 200
    color: palette.window
    BusyIndicator { x: 200; y: 20; running: true }
    Rectangle {
        x: 20; y: 40
        width: 40; height: 40
        radius: 20
        color: palette.highlight
        SequentialAnimation on y {
            loops: Animation.Infinite
            NumberAnimation { to: 80; duration: 600; easing.type: Easing.InOutQuad }
            NumberAnimation { to: 40; duration: 600; easing.type: Easing.InOutQuad }
        }
    }
}
