import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 200
    Dialog { id: renameDialog; title: qsTr("Rename"); standardButtons: Dialog.Ok | Dialog.Cancel }
    Menu {
        MenuItem { text: qsTr("Rename…"); onTriggered: renameDialog.open() }
        MenuItem { text: qsTr("Copy"); onTriggered: console.log("copied") }
    }
}
