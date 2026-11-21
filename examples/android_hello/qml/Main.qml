import QtQuick

// One window filling the phone's screen: a count that survives the
// activity going away (the home screen, a rotation, Back), and the
// screen's shape, which follows the phone as it turns.
Window {
    id: root
    objectName: "root"
    visible: true
    color: "#10141c"

    property int taps: 0
    readonly property bool wide: width > height

    Column {
        anchors.centerIn: parent
        spacing: 16

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            color: "white"
            font.pixelSize: 32
            text: "Tapped " + root.taps + (root.taps === 1 ? " time" : " times")
        }

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            color: "#9aa4b8"
            font.pixelSize: 18
            text: (root.wide ? "Landscape " : "Portrait ") + root.width + " × " + root.height
        }
    }

    MouseArea {
        anchors.fill: parent
        onClicked: root.taps++
    }
}
