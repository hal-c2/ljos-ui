import QtQuick

Item {
    id: root
    property var latest: ""
    property int seen: 0
    Timer {
        interval: 50
        repeat: true
        running: true
        // The latest update replaces the one before.
        onTriggered: {
            root.seen += 1
            root.latest = "x".repeat(100000) + root.seen
        }
    }
}
