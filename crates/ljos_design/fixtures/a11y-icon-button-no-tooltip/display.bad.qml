// expect: a11y-icon-button-no-tooltip at 7
import QtQuick
import QtQuick.Controls

Item {
    Button {
        icon.name: "document-save"
        text: qsTr("Save")
        display: AbstractButton.IconOnly
    }
}
