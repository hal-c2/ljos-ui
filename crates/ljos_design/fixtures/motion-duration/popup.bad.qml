// expect: motion-duration at 13
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
            duration: 600
            easing.type: Easing.OutCubic
        }
    }
}
