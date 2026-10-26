// expect: a11y-icon-button-no-tooltip at 7
import QtQuick
import QtQuick.Controls

Item {
    ToolButton {
        icon.name: "edit-copy"
        Accessible.name: qsTr("Copy")
    }
}
