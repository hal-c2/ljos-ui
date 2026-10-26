import QtQuick
import QtQuick.Controls

Item {
    width: 400; height: 120
    property int saved: 0
    Column {
        spacing: 8
        Button { text: qsTr("Save"); onClicked: saved++ }
        Text {
            id: statusLine
            text: saved > 0 ? qsTr("Saved %1 times").arg(saved) : qsTr("Not saved yet")
            Accessible.role: Accessible.StatusBar
            Accessible.name: text
        }
    }
}
