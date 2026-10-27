// expect: decl-visibility-juggling at 7
import QtQuick
import QtQuick.Controls

Column {
    Rectangle { id: banner; width: 80; height: 20; visible: false }
    Button { text: qsTr("Show"); onClicked: banner.visible = true }
    Button { text: qsTr("Hide"); onClicked: banner.visible = false }
}
