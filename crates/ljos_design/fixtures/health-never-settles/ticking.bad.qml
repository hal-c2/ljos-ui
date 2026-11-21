// expect: health-never-settles at 4
import QtQuick

Item {
    width: 200; height: 100
    Rectangle {
        id: dot
        width: 10; height: 10
    }
    // Moves the dot in every frame, with nothing animating.
    Timer {
        interval: 16
        repeat: true
        running: true
        onTriggered: dot.x = (dot.x + 1) % 190
    }
}
