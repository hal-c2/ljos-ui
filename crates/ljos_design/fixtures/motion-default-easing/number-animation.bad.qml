// expect: motion-default-easing at 8
import QtQuick

Rectangle {
    width: 100
    height: 40
    Behavior on x {
        NumberAnimation {
            duration: 200
        }
    }
}
