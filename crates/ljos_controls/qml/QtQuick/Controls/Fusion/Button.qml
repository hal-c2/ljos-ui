// The Fusion style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Fusion's own metrics and palette.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T

T.Button {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 4
    spacing: 6

    icon.width: 16
    icon.height: 16

    contentItem: Component {
        IconLabel {
            spacing: control.spacing
            mirrored: control.mirrored
            display: control.display
            text: control.text
            font: control.font
            color: control.highlighted ? control.palette.highlightedText : control.palette.buttonText
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 80
            implicitHeight: 24
            radius: 2
            visible: !control.flat || control.down || control.checked || control.highlighted
            color: control.down ? control.palette.mid
                 : control.highlighted ? control.palette.highlight
                 : control.checked ? control.palette.midlight
                 : control.palette.button
            border.color: control.visualFocus ? control.palette.highlight : control.palette.mid
            border.width: 1
        }
    }
}
