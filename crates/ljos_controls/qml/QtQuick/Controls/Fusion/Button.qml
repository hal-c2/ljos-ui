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
            color: control.palette.buttonText
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 80
            implicitHeight: 24
            radius: 2
            visible: !control.flat || control.down || control.checked || control.highlighted
            readonly property bool pressed: control.down || control.checked
            readonly property color button: control.palette.button
            readonly property color highlight: control.palette.highlight
            // DECISION: a highlighted button is tinted a tenth towards the
            // highlight; Qt's is a little bluer still.
            color: pressed ? Qt.rgba(button.r * 0.883, button.g * 0.883, button.b * 0.883, button.a)
                 : control.highlighted ? Qt.tint(Qt.darker(button, 1.03),
                                                 Qt.rgba(highlight.r, highlight.g, highlight.b, 0.1))
                 : Qt.darker(button, 1.03)
            gradient: Gradient {
                GradientStop {
                    position: 0
                    color: parent.pressed || control.highlighted ? parent.color
                                                                : Qt.lighter(control.palette.button, 1.07)
                }
                GradientStop {
                    position: 1
                    color: parent.pressed || control.highlighted ? parent.color
                                                                : Qt.darker(control.palette.button, 1.011)
                }
            }
            border.color: control.visualFocus ? control.palette.highlight
                        : control.highlighted ? Qt.darker(control.palette.highlight, 1.25)
                        : control.enabled ? Qt.darker(control.palette.button, 1.4)
                        : Qt.darker(control.palette.button, 1.22)
            border.width: 1
        }
    }
}
