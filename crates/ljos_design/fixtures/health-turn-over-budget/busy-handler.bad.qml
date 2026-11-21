// expect: health-turn-over-budget at 11
import QtQuick

Item {
    id: root
    property int total: 0
    Timer {
        interval: 100
        running: true
        // Sums ten million numbers in one turn: the window freezes.
        onTriggered: {
            var n = 0
            for (var i = 0; i < 10000000; i++)
                n += i % 7
            root.total = n
        }
    }
}
