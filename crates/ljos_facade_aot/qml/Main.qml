import QtQuick
import Example

Item {
    Counter {
        id: counter
        count: 41
        onBumped: root.bumps++
    }
    id: root
    property int bumps: 0
    property int seen: counter.count + 1
}
