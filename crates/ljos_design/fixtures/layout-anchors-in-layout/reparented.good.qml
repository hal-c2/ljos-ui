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
    // Drawn inside the field's right end, so the layout does not place it.
    Button {
        parent: field
        anchors.right: parent.right
        text: qsTr("Add")
    }
}
