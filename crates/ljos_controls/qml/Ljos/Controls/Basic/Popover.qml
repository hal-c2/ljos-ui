// Ljos.Controls.Basic's popover (docs/DIALECT.md §4.16): placed as
// Ljos.Controls' is (Ljos.Controls.impl's PopoverBase) and drawn as the
// Basic style draws a popup.
import QtQuick
import QtQuick.Templates as T
import Ljos.Controls.impl

PopoverBase {
    id: control

    T.Overlay.modal: Rectangle {
        color: Qt.alpha(control.palette.shadow, 0.5)
    }
    T.Overlay.modeless: Rectangle {
        color: Qt.alpha(control.palette.shadow, 0.12)
    }

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    margins: 12
    padding: 12

    background: Component {
        Rectangle {
            color: control.palette.window
            border.color: control.palette.dark
        }
    }
}
