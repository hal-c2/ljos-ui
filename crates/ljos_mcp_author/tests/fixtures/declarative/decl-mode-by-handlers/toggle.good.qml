import QtQuick
import QtQuick.Controls

Column {
    property bool open: false
    Button { text: qsTr("Toggle"); onClicked: open = !open }
    Button { text: qsTr("Close"); onClicked: open = false }
}
