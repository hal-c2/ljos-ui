// expect: motion-duration at 9
import QtQuick

Rectangle {
    width: 100
    height: 40
    Behavior on opacity {
        NumberAnimation {
            duration: 30
            easing.type: Easing.OutCubic
        }
    }
}
