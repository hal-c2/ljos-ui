import QtQuick

Item {
    id: root
    property int total: 0
    Timer {
        interval: 100
        running: true
        // A thousand numbers: a turn's worth.
        onTriggered: {
            var n = 0
            for (var i = 0; i < 1000; i++)
                n += i % 7
            root.total = n
        }
    }
}
