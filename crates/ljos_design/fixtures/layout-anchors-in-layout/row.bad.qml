// expect: layout-anchors-in-layout at 12
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

RowLayout {
    width: 300
    Label {
        text: qsTr("Name")
    }
    TextField {
        anchors.right: parent.right
        Accessible.name: qsTr("Name")
    }
}
