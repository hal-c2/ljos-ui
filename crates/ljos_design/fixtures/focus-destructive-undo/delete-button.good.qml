import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 80
    Dialog {
        id: confirmDialog
        title: qsTr("Delete project")
        standardButtons: Dialog.Ok | Dialog.Cancel
        Button { text: qsTr("Delete everything"); onClicked: console.log("deleted") }
    }
    Button { text: qsTr("Delete project"); onClicked: confirmDialog.open() }
}
