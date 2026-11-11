// The Material style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Material's own metrics and attached palette.
import QtQuick
import QtQuick.Controls.Material
import QtQuick.Templates as T

T.Popup {
    id: control

    // What is under the popup while it dims it: a dark shade in the light
    // theme and a light one in the dark, modal or not. It fades in as the
    // popup opens and out as it closes.
    T.Overlay.modal: Rectangle {
        color: control.Material.theme === Material.Dark ? "#99fafafa" : "#99303030"
        Behavior on opacity { NumberAnimation { duration: 150 } }
    }
    T.Overlay.modeless: Rectangle {
        color: control.Material.theme === Material.Dark ? "#99fafafa" : "#99303030"
        Behavior on opacity { NumberAnimation { duration: 150 } }
    }

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    padding: 12

    background: Component {
        Rectangle {
            radius: 4
            color: control.Material.dialogColor
            border.color: control.Material.dividerColor
        }
    }
}
