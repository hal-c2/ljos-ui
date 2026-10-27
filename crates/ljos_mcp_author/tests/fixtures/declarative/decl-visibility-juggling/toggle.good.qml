import QtQuick
import QtQuick.Controls

Column {
    Rectangle { id: banner; width: 80; height: 20; visible: false }
    Button { text: qsTr("Toggle"); onClicked: banner.visible = !banner.visible }
}
