// A ListView of 10 000 delegates scrolled from the first to the last and
// back, again and again: delegates made and dropped as they pass.
import QtQuick

Rectangle {
    width: 400
    height: 600
    color: "white"

    ListView {
        id: list
        anchors.fill: parent
        model: 10000
        delegate: Rectangle {
            required property int index
            width: ListView.view.width
            height: 40
            color: index % 2 ? "#eeeeee" : "white"
            Text {
                anchors.verticalCenter: parent.verticalCenter
                x: 12
                text: "Row " + parent.index
            }
        }

        SequentialAnimation on contentY {
            loops: Animation.Infinite
            NumberAnimation { from: 0; to: 10000 * 40 - 600; duration: 20000 }
            NumberAnimation { from: 10000 * 40 - 600; to: 0; duration: 20000 }
        }
    }
}
