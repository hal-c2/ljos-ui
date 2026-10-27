import QtQuick
import QtQuick.Controls

Item {
    Popup { id: popup }
    Button { text: qsTr("Open"); onClicked: popup.visible = true }
    Button { text: qsTr("Close"); onClicked: popup.visible = false }
}
