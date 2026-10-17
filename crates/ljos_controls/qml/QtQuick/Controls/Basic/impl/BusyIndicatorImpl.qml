// The ring the Basic busy indicator is drawn with, written from the Qt
// Quick Controls documentation and from pictures of a running Qt
// (CLEAN_ROOM.md).
import QtQuick

Item {
    id: ring

    property color pen
    property color fill
    property bool running

    // Twenty steps of a tenth of a second: ten that fill the dots from the
    // top, ten that empty them in the same order. A ring that is not
    // running holds half of them, as Qt draws it then.
    QtObject {
        id: turning
        property int step: 0
        readonly property int shown: ring.running ? step : 4
    }

    NumberAnimation {
        target: turning
        property: "step"
        from: 0
        to: 20
        duration: 2000
        loops: Animation.Infinite
        running: ring.running && ring.visible && ring.opacity > 0
    }

    Repeater {
        model: 10

        Rectangle {
            required property int index
            readonly property real turn: index / 10 * 2 * Math.PI
            readonly property real reach: (Math.min(ring.width, ring.height) - width) / 2

            x: ring.width / 2 - width / 2 + Math.sin(turn) * reach
            y: ring.height / 2 - height / 2 - Math.cos(turn) * reach
            width: Math.min(ring.width, ring.height) / 6
            height: width
            radius: width / 2
            border.width: 1
            border.color: ring.pen
            color: (turning.shown < 10 ? index <= turning.shown : index > turning.shown - 10)
                   ? ring.fill : "transparent"
        }
    }
}
