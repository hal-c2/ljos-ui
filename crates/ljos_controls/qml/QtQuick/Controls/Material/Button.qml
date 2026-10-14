// The Material style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Material's own metrics and attached palette.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Controls.Material
import QtQuick.Templates as T

T.Button {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    topInset: 6
    bottomInset: 6
    topPadding: 14
    bottomPadding: 14
    leftPadding: 24
    rightPadding: 16
    spacing: 8

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
            implicitWidth: 64
            implicitHeight: 40
            radius: 4
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
