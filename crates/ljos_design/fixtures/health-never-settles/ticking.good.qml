import QtQuick

Item {
    width: 200; height: 100
    Rectangle {
        id: dot
        width: 10; height: 10
    }
    // Once a second, as a clock does.
    Timer {
        interval: 1000
        repeat: true
        running: true
        onTriggered: dot.x = (dot.x + 1) % 190
    }
}
