// The Fusion style of Ljós (CLEAN_ROOM.md), drawn with its parts in
// QtQuick.Controls.Fusion.impl: drawings of our own, fitted to what qml6 shows.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Controls.Fusion
import QtQuick.Controls.Fusion.impl
import QtQuick.Templates as T

T.RoundButton {
    // Not `control`: inside a part's component that names the part's
    // own property, which Fusion's parts have.
    id: host

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 6
    spacing: 6

    icon.width: 16
    icon.height: 16

    contentItem: Component {
        IconLabel {
            spacing: host.spacing
            mirrored: host.mirrored
            display: host.display
            text: host.text
            font: host.font
            color: host.palette.buttonText
        }
    }

    background: Component {
        ButtonPanel {
            id: panel
            implicitWidth: 32
            implicitHeight: 32
            control: host
            radius: host.radius
            visible: !host.flat || host.down || host.checked || host.highlighted
            pressed: host.down || host.checked
            highlighted: host.highlighted
            // Unlike the other buttons, always a gradient over the
            // rectangle's own white: flat in the pressed shade while down
            // (fusion-states.feature).
            color: "#ffffff"
            gradient: Gradient {
                GradientStop {
                    position: 0
                    color: panel.pressed ? Fusion.buttonColor(host.palette, host.highlighted, true, panel.hovered)
                                         : Fusion.gradientStart(panel.face)
                }
                GradientStop {
                    position: 1
                    color: panel.pressed ? Fusion.buttonColor(host.palette, host.highlighted, true, panel.hovered)
                                         : Fusion.gradientStop(panel.face)
                }
            }
        }
    }
}
