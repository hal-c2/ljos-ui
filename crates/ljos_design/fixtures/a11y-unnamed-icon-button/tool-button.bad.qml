// expect: a11y-unnamed-icon-button at 6
import QtQuick
import QtQuick.Controls

Item {
    ToolButton {
        icon.name: "edit-copy"
        ToolTip.text: qsTr("Copy")
        ToolTip.visible: hovered
    }
}
