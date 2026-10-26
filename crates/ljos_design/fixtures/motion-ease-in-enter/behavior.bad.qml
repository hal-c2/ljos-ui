// expect: motion-ease-in-enter at 11
import QtQuick

Rectangle {
    width: 100
    height: 40
    opacity: 0.5
    Behavior on opacity {
        NumberAnimation {
            duration: 150
            easing.type: Easing.InQuad
        }
    }
}
