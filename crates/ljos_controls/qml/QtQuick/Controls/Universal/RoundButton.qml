// The Universal style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Universal's own metrics and attached palette.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Controls.Universal
import QtQuick.Templates as T

T.RoundButton {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 8
    spacing: 8

    icon.width: 16
    icon.height: 16

    contentItem: Component {
        IconLabel {
            spacing: control.spacing
            mirrored: control.mirrored
            display: control.display
            text: control.text
            font: control.font
            color: control.Universal.foreground
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 32
            implicitHeight: 32
            radius: control.radius
            visible: !control.flat || control.down || control.checked || control.highlighted
            color: control.down ? control.Universal.chromeMediumColor
                 : control.highlighted ? control.Universal.accent
                 : control.checked ? control.Universal.chromeMediumColor
                 : control.Universal.baseLowColor
            border.color: control.visualFocus ? control.Universal.foreground : "transparent"
            border.width: control.visualFocus ? 2 : 0
        }
    }
}
