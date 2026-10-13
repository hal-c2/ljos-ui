// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T

T.RoundButton {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 6
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
            color: control.checked || control.highlighted ? control.palette.brightText
                                                          : control.palette.buttonText
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 40
            implicitHeight: 40
            radius: control.radius
            visible: !control.flat || control.down || control.checked || control.highlighted
            color: control.down ? control.palette.mid
                   : control.checked || control.highlighted ? control.palette.dark
                                                            : control.palette.button
            border.color: control.palette.highlight
            border.width: control.visualFocus ? 2 : 0
        }
    }
}
