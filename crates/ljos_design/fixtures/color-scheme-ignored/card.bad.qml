// expect: color-scheme-ignored at 8
import QtQuick

Rectangle {
    width: 200
    height: 80
    // ljos-allow(color-literal): the defect this fixture is about
    color: "#ffffff"
    Text {
        anchors.centerIn: parent
        text: qsTr("Card")
        // ljos-allow(color-literal): the defect this fixture is about
        color: "#202020"
    }
}
