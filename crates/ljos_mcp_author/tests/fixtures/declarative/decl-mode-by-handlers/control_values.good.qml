import QtQuick
import QtQuick.Controls

Column {
    Slider { id: first; Accessible.name: qsTr("First") }
    Slider { id: second; Accessible.name: qsTr("Second") }
    Button { text: qsTr("Low"); onClicked: { first.value = 0.1; second.value = 0.2 } }
    Button { text: qsTr("High"); onClicked: { first.value = 0.9; second.value = 0.8 } }
}
