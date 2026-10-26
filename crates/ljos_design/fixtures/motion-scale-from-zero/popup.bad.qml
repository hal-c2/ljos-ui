// expect: motion-scale-from-zero at 11
import QtQuick
import QtQuick.Controls

Popup {
    width: 200
    height: 120
    enter: Transition {
        NumberAnimation {
            property: "scale"
            from: 0
            to: 1
            duration: 150
            easing.type: Easing.OutCubic
        }
    }
}
