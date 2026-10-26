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
            duration: 250
            easing.type: Easing.OutCubic
        }
    }
}
