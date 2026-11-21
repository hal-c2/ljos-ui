// expect: health-heap-growth at 4
import QtQuick

Item {
    id: root
    property var history: ({ updates: [] })
    property int seen: 0
    Timer {
        interval: 50
        repeat: true
        running: true
        // Every update is kept, and none is let go.
        onTriggered: {
            root.seen += 1
            root.history.updates.push("x".repeat(100000) + root.seen)
        }
    }
}
