import QtQuick
import QtQuick.Controls

Column {
    property bool shown: false
    Rectangle { width: 80; height: 20; visible: shown }
    Button { text: qsTr("Show"); onClicked: shown = true }
    Button { text: qsTr("Hide"); onClicked: shown = false }
}
