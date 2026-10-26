// expect: a11y-unnamed-interactive at 12
import QtQuick
import QtQuick.Controls

Rectangle {
    width: 200
    height: 40
    Label {
        anchors.centerIn: parent
        text: qsTr("report.txt")
    }
    TapHandler {
        onTapped: menu.popup()
    }
    Menu {
        id: menu
        MenuItem { text: qsTr("Rename") }
    }
}
