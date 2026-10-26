import QtQuick

Rectangle {
    width: 100
    height: 40
    Behavior on x {
        NumberAnimation {
            duration: 200
            easing.type: Easing.InOutCubic
        }
    }
}
