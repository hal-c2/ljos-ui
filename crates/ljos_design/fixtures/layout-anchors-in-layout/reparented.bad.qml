// expect: layout-anchors-in-layout at 14
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    width: 300
    TextField {
        id: field
        Layout.fillWidth: true
        Accessible.name: qsTr("Path")
    }
    Button {
        anchors.right: parent.right
        text: qsTr("Add")
    }
}
