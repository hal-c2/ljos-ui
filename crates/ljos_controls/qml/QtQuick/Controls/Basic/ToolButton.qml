// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T

T.ToolButton {
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
            color: control.palette.buttonText
        }
    }

    // Faint until it is held down, so that a row of them reads as a bar.
    background: Component {
        Rectangle {
            implicitWidth: 40
            implicitHeight: 40
            opacity: control.down ? 1.0 : 0.5
            color: control.down || control.checked || control.highlighted ? control.palette.mid
                                                                          : control.palette.button
            // Where the keyboard is, when a key brought it there.
            border.color: control.palette.highlight
            border.width: control.visualFocus ? 2 : 0
        }
    }
}
