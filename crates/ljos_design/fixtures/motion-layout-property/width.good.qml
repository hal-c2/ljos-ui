import QtQuick

Rectangle {
    width: 200
    height: 100
    Behavior on scale {
        NumberAnimation {
            duration: 200
            easing.type: Easing.OutCubic
        }
    }
}
