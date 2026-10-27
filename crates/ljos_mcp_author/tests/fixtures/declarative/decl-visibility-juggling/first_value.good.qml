import QtQuick
import QtQuick.Controls

Column {
    Rectangle { id: banner; width: 80; height: 20 }
    Component.onCompleted: banner.visible = false
    Button { text: qsTr("Show"); onClicked: banner.visible = true }
}
