// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports.
import QtQuick
import QtQuick.Templates as T

T.SpinBox {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentItem.implicitWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             up.implicitIndicatorHeight, down.implicitIndicatorHeight)

    padding: 4
    // Both steps are in a strip at one side.
    leftPadding: padding + (control.mirrored ? 16 : 0)
    rightPadding: padding + (control.mirrored ? 0 : 16)

    // Above the buttons, so that a press on the number is the number's.
    contentItem: Component {
        TextInput {
            z: 2
            text: control.displayText

            font: control.font
            color: control.palette.text
            selectionColor: control.palette.highlight
            selectedTextColor: control.palette.highlightedText
            horizontalAlignment: Qt.AlignHCenter
            verticalAlignment: Qt.AlignVCenter

            readOnly: !control.editable
            validator: control.validator
            inputMethodHints: control.inputMethodHints

            // The box speaks for its number: one thing with a value, a name
            // and steps, and not a nameless field inside it.
            Accessible.ignored: true

            // The frame of the box that has the keyboard.
            Rectangle {
                width: parent.width
                height: parent.height
                visible: control.activeFocus
                color: "transparent"
                border.color: control.palette.highlight
                border.width: 2
            }
        }
    }

    up.indicator: Component {
        Rectangle {
            x: control.mirrored ? 1 : control.width - width - 1
            y: 1
            implicitWidth: 16
            implicitHeight: 10
            height: control.height / 2 - 1
            color: control.up.pressed ? Qt.darker(control.palette.button, 1.1) : "transparent"

            // A plus sign: two bars of our own.
            Rectangle {
                x: (parent.width - width) / 2
                y: (parent.height - height) / 2
                width: 7
                height: 1
                color: enabled ? control.palette.buttonText : control.palette.mid
            }

            Rectangle {
                x: (parent.width - width) / 2
                y: (parent.height - height) / 2
                width: 1
                height: 7
                color: enabled ? control.palette.buttonText : control.palette.mid
            }
        }
    }

    down.indicator: Component {
        Rectangle {
            x: control.mirrored ? 1 : control.width - width - 1
            y: control.height / 2
            implicitWidth: 16
            implicitHeight: 10
            height: control.height / 2 - 1
            color: control.down.pressed ? Qt.darker(control.palette.button, 1.1) : "transparent"

            // A minus sign.
            Rectangle {
                x: (parent.width - width) / 2
                y: (parent.height - height) / 2
                width: 7
                height: 1
                color: enabled ? control.palette.buttonText : control.palette.mid
            }
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 120
            implicitHeight: 24
            radius: 2
            color: control.palette.base
            border.color: control.activeFocus ? control.palette.highlight
                                              : Qt.darker(control.palette.button, 1.4)
        }
    }
}
