import QtQuick
import QtQuick.Controls

Rectangle {
    width: 200
    height: 40
    Label {
        anchors.centerIn: parent
        text: qsTr("report.txt")
    }
    // The secondary button is a second way to what the row's own menu
    // offers; the plain way in is what has to be named.
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: menu.popup()
    }
    Menu {
        id: menu
        MenuItem { text: qsTr("Rename") }
    }
}
