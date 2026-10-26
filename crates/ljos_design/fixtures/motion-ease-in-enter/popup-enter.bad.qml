// expect: motion-ease-in-enter at 14
import QtQuick
import QtQuick.Controls

Popup {
    width: 200
    height: 120
    enter: Transition {
        NumberAnimation {
            property: "opacity"
            from: 0
            to: 1
            duration: 150
            easing.type: Easing.InCubic
        }
    }
}
