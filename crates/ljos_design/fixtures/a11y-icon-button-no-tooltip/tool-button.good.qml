import QtQuick
import QtQuick.Controls

Item {
    ToolButton {
        icon.name: "edit-copy"
        Accessible.name: qsTr("Copy")
        ToolTip.text: qsTr("Copy")
        ToolTip.visible: hovered
    }
    Button {
        icon.name: "document-save"
        text: qsTr("Save")
    }
}
