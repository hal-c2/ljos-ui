// expect: motion-frequent at 12
import QtQuick

Rectangle {
    id: row
    width: 200
    height: 30
    focus: true
    opacity: row.activeFocus ? 1 : 0.6
    Behavior on opacity {
        NumberAnimation {
            duration: 250
            easing.type: Easing.OutCubic
        }
    }
}
