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
        // Pulses instead of moving: a fade is what reduced motion keeps.
        SequentialAnimation on opacity {
            loops: Animation.Infinite
            NumberAnimation { to: 0.4; duration: 600; easing.type: Easing.InOutQuad }
            NumberAnimation { to: 1; duration: 600; easing.type: Easing.InOutQuad }
        }
    }
}
