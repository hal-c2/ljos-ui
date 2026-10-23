// The Fusion style of Ljós (CLEAN_ROOM.md), drawn with its parts in
// QtQuick.Controls.Fusion.impl: drawings of our own, fitted to what qml6 shows.
import QtQuick
import QtQuick.Controls.Fusion
import QtQuick.Controls.Fusion.impl
import QtQuick.Shapes
import QtQuick.Templates as T

T.SpinBox {
    // Not `control`: inside a part's component that names the part's
    // own property, which Fusion's parts have.
    id: host

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentItem.implicitWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             up.implicitIndicatorHeight, down.implicitIndicatorHeight)

    padding: 4
    // Both steps are in a strip at one side.
    leftPadding: padding + (host.mirrored ? 16 : 0)
    rightPadding: padding + (host.mirrored ? 0 : 16)

    // Above the buttons, so that a press on the number is the number's.
    contentItem: Component {
        TextInput {
            z: 2
            text: host.displayText

            font: host.font
            color: host.palette.text
            selectionColor: host.palette.highlight
            selectedTextColor: host.palette.highlightedText
            horizontalAlignment: Qt.AlignHCenter
            verticalAlignment: Qt.AlignVCenter

            readOnly: !host.editable
            validator: host.validator
            inputMethodHints: host.inputMethodHints

            // The box speaks for its number: one thing with a value, a name
            // and steps, and not a nameless field inside it.
            Accessible.ignored: true
        }
    }

    // A step is pale where it can go no further.
    up.indicator: Component {
        Rectangle {
            x: host.mirrored ? 1 : host.width - width - 1
            y: 1
            implicitWidth: 16
            implicitHeight: 10
            height: host.height / 2 - 1
            radius: 1.7
            readonly property color base: host.palette.base
            color: host.up.pressed ? Qt.rgba(base.r * 0.86, base.g * 0.86, base.b * 0.86, base.a)
                                   : "transparent"

            // A small filled triangle of our own (CLEAN_ROOM.md), eight wide and
            // four tall, in the text colour at about 72 % as qml6 shows it;
            // where the step's down arrow would be turned about its strip.
            Shape {
                id: upArrow
                property color color: host.palette.buttonText
                x: parent.width - Math.round((parent.width - 8) / 2) - 8
                y: parent.height - Math.round((parent.height - 4) / 2) - 4
                width: 8
                height: 4
                opacity: parent.enabled ? 1 : 0.5
                ShapePath {
                    strokeWidth: -1
                    fillColor: Qt.rgba(upArrow.color.r, upArrow.color.g, upArrow.color.b, upArrow.color.a * 0.72)
                    PathSvg { path: "M 0 4 L 8 4 L 4 0 Z" }
                }
            }
        }
    }

    down.indicator: Component {
        Rectangle {
            x: host.mirrored ? 1 : host.width - width - 1
            y: host.height / 2
            implicitWidth: 16
            implicitHeight: 10
            height: host.height / 2 - 1
            radius: 1.7
            readonly property color base: host.palette.base
            color: host.down.pressed ? Qt.rgba(base.r * 0.86, base.g * 0.86, base.b * 0.86, base.a)
                                     : "transparent"

            Shape {
                id: downArrow
                property color color: host.palette.buttonText
                x: Math.round((parent.width - 8) / 2)
                y: Math.round((parent.height - 4) / 2)
                width: 8
                height: 4
                opacity: parent.enabled ? 1 : 0.5
                ShapePath {
                    strokeWidth: -1
                    fillColor: Qt.rgba(downArrow.color.r, downArrow.color.g, downArrow.color.b, downArrow.color.a * 0.72)
                    PathSvg { path: "M 0 0 L 8 0 L 4 4 Z" }
                }
            }
        }
    }

    // A sunken field with a raised strip for the steps, a line at its side.
    background: Component {
        TextFieldBackground {
            control: host

            Rectangle {
                x: host.mirrored ? 1 : parent.width - width - 1
                y: 1
                width: 17
                height: parent.height - 2
                radius: 2
                gradient: Gradient {
                    GradientStop {
                        position: 0
                        color: Fusion.gradientStart(Fusion.buttonColor(host.palette, false, false, false))
                    }
                    GradientStop {
                        position: 1
                        color: Fusion.gradientStop(Fusion.buttonColor(host.palette, false, false, false))
                    }
                }

                Rectangle {
                    x: host.mirrored ? parent.width - 1 : 0
                    width: 1
                    height: parent.height
                    color: Fusion.outline(host.palette)
                }
            }
        }
    }
}
