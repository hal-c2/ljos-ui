import QtQuick
import QtQuick.Controls

Item {
    Button {
        icon.name: "document-save"
        text: qsTr("Save")
        display: AbstractButton.IconOnly
        ToolTip.text: text
        ToolTip.visible: hovered || visualFocus
    }
}
