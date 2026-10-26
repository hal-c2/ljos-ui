// expect: motion-endless at 11
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 120; height: 40
    Rectangle {
        width: 100; height: 12
        color: palette.mid
        SequentialAnimation on opacity { loops: Animation.Infinite
            NumberAnimation { to: 0.5; duration: 1000 }
            NumberAnimation { to: 1; duration: 1000 }
        }
    }
}
