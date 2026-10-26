import QtQuick

Rectangle {
    width: 100
    height: 40
    Behavior on opacity {
        NumberAnimation {
            duration: 0
        }
    }
}
