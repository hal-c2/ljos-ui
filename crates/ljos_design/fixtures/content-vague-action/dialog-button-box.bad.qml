// expect: content-vague-action at 9
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 240; height: 120
    DialogButtonBox {
        Button { text: qsTr("Continue"); DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole }
        Button { text: qsTr("Cancel"); DialogButtonBox.buttonRole: DialogButtonBox.RejectRole }
    }
}
