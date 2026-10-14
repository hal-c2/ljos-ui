// The Material style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Material's own metrics and attached palette.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Controls.Material
import QtQuick.Templates as T

T.RoundButton {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    topInset: 6
    bottomInset: 6
    leftInset: 6
    rightInset: 6
    padding: 12
    spacing: 6

    icon.width: 24
    icon.height: 24

    contentItem: Component {
        IconLabel {
            spacing: control.spacing
            mirrored: control.mirrored
            display: control.display
            text: control.text
            font: control.font
            color: control.Material.foreground
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 40
            implicitHeight: 40
            radius: control.radius
            visible: !control.flat || control.down || control.checked || control.highlighted
            color: control.Material.buttonColor(
                control.Material.theme,
                control.Material.background,
                control.Material.accent,
                control.enabled,
                control.flat,
                control.highlighted,
                control.checked
            )
            border.color: control.palette.highlight
            border.width: control.visualFocus ? 2 : 0
        }
    }
}
