// 200 NumberAnimations running at once, each moving its own Rectangle:
// the cost of animating, frame after frame.
import QtQuick

Rectangle {
    width: 800
    height: 600
    color: "white"

    Repeater {
        model: 200
        Rectangle {
            required property int index
            y: index * 3
            width: 12
            height: 3
            color: Qt.hsla((index % 50) / 50, 0.7, 0.5, 1)

            NumberAnimation on x {
                from: 0
                to: 788
                duration: 1000 + (index % 20) * 100
                loops: Animation.Infinite
            }
        }
    }
}
