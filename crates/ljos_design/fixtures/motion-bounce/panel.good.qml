import QtQuick

Rectangle {
    width: 200
    height: 100
    Behavior on y {
        NumberAnimation {
            duration: 300
            easing.type: Easing.OutCubic
        }
    }
}
