import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 80
    property int period: 800
    Rectangle {
        width: 40; height: 40
        color: palette.window
        SequentialAnimation on color {
            loops: Animation.Infinite
            running: false
            ColorAnimation { to: palette.highlight; duration: period }
            ColorAnimation { to: palette.window; duration: period }
        }
    }
}
