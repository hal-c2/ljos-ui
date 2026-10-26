// expect: motion-layout-property at 8
import QtQuick

Rectangle {
    width: 200
    height: 100
    Behavior on width {
        NumberAnimation {
            duration: 200
            easing.type: Easing.OutCubic
        }
    }
}
