// expect: a11y-icon-button-no-tooltip at 10, a11y-icon-button-no-tooltip at 11
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 120
    Row {
        spacing: 8
        ToolButton { icon.name: "edit-copy"; display: AbstractButton.IconOnly; Accessible.name: "Copy" }
        ToolButton { icon.name: "edit-paste"; display: AbstractButton.IconOnly; Accessible.name: "Paste" }
    }
}
