// expect: motion-flash at 11
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 80
    Rectangle {
        width: 40; height: 40
        color: palette.window
        SequentialAnimation on color { loops: Animation.Infinite
            ColorAnimation { to: palette.highlight; duration: 120 }
            ColorAnimation { to: palette.window; duration: 120 }
        }
    }
}
