import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

RowLayout {
    width: 300
    Label {
        text: qsTr("Name")
    }
    Item {
        Layout.fillWidth: true
        implicitHeight: field.implicitHeight
        TextField {
            id: field
            anchors.right: parent.right
            Accessible.name: qsTr("Name")
        }
    }
}
