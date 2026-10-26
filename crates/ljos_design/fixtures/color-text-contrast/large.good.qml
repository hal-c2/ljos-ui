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
            font.pixelSize: 24
            // ljos-allow(color-literal): a fixed badge, the same in every scheme
            color: "#888888"
        }
    }
}
