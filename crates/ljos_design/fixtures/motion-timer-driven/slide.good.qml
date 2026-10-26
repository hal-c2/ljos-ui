import QtQuick

Item {
    width: 300
    height: 100
    Rectangle {
        id: box
        width: 20
        height: 20
        NumberAnimation on x {
            to: 280
            duration: 300
            easing.type: Easing.InOutCubic
        }
    }
    Timer {
        interval: 1000
        repeat: true
        running: true
        onTriggered: console.log("tick")
    }
}
