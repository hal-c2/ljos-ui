import QtQuick
import QtQuick.Controls

Item {
    width: 500; height: 400
    Dialog {
        x: 50; y: 50; width: 300; height: 200
        modal: true
        title: qsTr("Reload the file?")
        standardButtons: Dialog.Ok | Dialog.Cancel
        Label { text: qsTr("The file has changed on disk.") }
        Component.onCompleted: open()
    }
}
