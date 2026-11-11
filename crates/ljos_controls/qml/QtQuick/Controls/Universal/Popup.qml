// The Universal style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Universal's own metrics and attached palette.
import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Templates as T

T.Popup {
    id: control

    // What is under the popup while it dims it: the theme's base low
    // colour, modal or not.
    T.Overlay.modal: Rectangle {
        color: control.Universal.baseLowColor
    }
    T.Overlay.modeless: Rectangle {
        color: control.Universal.baseLowColor
    }

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    padding: 12

    background: Component {
        Rectangle {
            color: control.Universal.background
            border.color: control.Universal.chromeHighColor
        }
    }
}
