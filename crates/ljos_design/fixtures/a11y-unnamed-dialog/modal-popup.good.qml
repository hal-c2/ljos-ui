import QtQuick
import QtQuick.Controls

Item {
    width: 500; height: 400
    Popup {
        x: 50; y: 50; width: 300; height: 200
        modal: true
        Accessible.name: qsTr("Sharing")
        Button { text: qsTr("Close"); onClicked: parent.close() }
        Component.onCompleted: open()
    }
}
