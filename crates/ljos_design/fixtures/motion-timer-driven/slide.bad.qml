// expect: motion-timer-driven at 12
import QtQuick

Item {
    width: 300
    height: 100
    Rectangle {
        id: box
        width: 20
        height: 20
    }
    Timer {
        interval: 16
        repeat: true
        running: true
        onTriggered: box.x += 2
    }
}
