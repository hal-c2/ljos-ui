import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 120
    Row {
        spacing: 8
        ToolButton {
            icon.name: "edit-copy"; display: AbstractButton.IconOnly; Accessible.name: "Copy"
            ToolTip.visible: hovered; ToolTip.text: "Copy"
        }
        ToolButton {
            icon.name: "edit-paste"; display: AbstractButton.IconOnly; Accessible.name: "Paste"
            ToolTip.visible: hovered; ToolTip.text: "Paste"
        }
    }
}
