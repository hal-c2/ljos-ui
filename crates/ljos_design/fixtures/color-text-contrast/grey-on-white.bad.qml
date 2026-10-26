// expect: color-text-contrast at 16
import QtQuick

Rectangle {
    width: 200
    height: 40
    color: palette.base
    Rectangle {
        anchors.fill: parent
        // ljos-allow(color-literal): a fixed badge, the same in every scheme
        color: "#ffffff"
        Text {
            anchors.centerIn: parent
            text: qsTr("Draft")
            // ljos-allow(color-literal): a fixed badge, the same in every scheme
            color: "#aaaaaa"
        }
    }
}
