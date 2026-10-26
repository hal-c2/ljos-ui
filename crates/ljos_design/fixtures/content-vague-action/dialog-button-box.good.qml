import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 240; height: 120
    DialogButtonBox {
        Button { text: qsTr("Send report"); DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole }
        Button { text: qsTr("Cancel"); DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
    }
}
