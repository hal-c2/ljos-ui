// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports. The current tab is the full height of the
// bar; the others start lower, behind it.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T

T.TabButton {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 2
    horizontalPadding: 4
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
            color: control.palette.buttonText
        }
    }

    // DECISION: the shading of a tab is Ljós's own; Qt reports only its
    // frame. The current one is lighter at the top, like a raised button.
    background: Component {
        Rectangle {
            y: control.checked ? 0 : 2
            implicitHeight: 21
            height: control.height - y
            border.color: Qt.darker(control.palette.window, 1.27)
            gradient: Gradient {
                GradientStop {
                    position: 0
                    color: control.checked ? Qt.lighter(control.palette.window, 1.1)
                                           : Qt.darker(control.palette.window, 1.02)
                }
                GradientStop {
                    position: 1
                    color: control.checked ? control.palette.window
                                           : Qt.darker(control.palette.window, control.down ? 1.1 : 1.05)
                }
            }
        }
    }
}
