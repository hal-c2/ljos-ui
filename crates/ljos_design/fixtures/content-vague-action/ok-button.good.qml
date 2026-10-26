import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 80
    Button { text: qsTr("Save draft"); onClicked: console.log("saved") }
    Dialog { standardButtons: Dialog.Ok | Dialog.Cancel }
    RadioButton { text: qsTr("Yes") }
}
