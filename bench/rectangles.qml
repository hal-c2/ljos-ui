// 1000 Rectangles made by a Repeater, laid out in a grid: the cost of
// many plain items, made once and then left alone.
import QtQuick

Rectangle {
    width: 800
    height: 600
    color: "white"

    Repeater {
        model: 1000
        Rectangle {
            required property int index
            x: (index % 40) * 20
            y: Math.floor(index / 40) * 24
            width: 18
            height: 22
            color: Qt.hsla((index % 97) / 97, 0.6, 0.5, 1)
            radius: index % 3
        }
    }
}
