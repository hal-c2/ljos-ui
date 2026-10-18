// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.SpinBox {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentItem.implicitWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             up.implicitIndicatorHeight, down.implicitIndicatorHeight)

    // The number keeps clear of the button at either side.
    leftPadding: padding + (control.mirrored
                            ? (up.indicator ? up.indicator.width : 0)
                            : (down.indicator ? down.indicator.width : 0))
    rightPadding: padding + (control.mirrored
                             ? (down.indicator ? down.indicator.width : 0)
                             : (up.indicator ? up.indicator.width : 0))

    // Above the buttons, so that a press on the number is the number's.
    contentItem: Component {
        TextInput {
            z: 2
            text: control.displayText
            padding: 6

            font: control.font
            color: control.palette.text
            selectionColor: control.palette.highlight
            selectedTextColor: control.palette.highlightedText
            horizontalAlignment: Qt.AlignHCenter
            verticalAlignment: Qt.AlignVCenter

            readOnly: !control.editable
            validator: control.validator
            inputMethodHints: control.inputMethodHints

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
            x: control.mirrored ? 0 : control.width - width
            height: control.height
            implicitWidth: 40
            implicitHeight: 40
            color: control.up.pressed ? control.palette.mid : control.palette.button
            border.color: enabled ? control.palette.text : control.palette.mid
            border.width: 0

            // A plus sign: two bars of our own.
            Rectangle {
                x: (parent.width - width) / 2
                y: (parent.height - height) / 2
                width: parent.width / 3
                height: 2
                color: enabled ? control.palette.buttonText : control.palette.mid
            }

            Rectangle {
                x: (parent.width - width) / 2
                y: (parent.height - height) / 2
                width: 2
                height: parent.width / 3
                color: enabled ? control.palette.buttonText : control.palette.mid
            }
        }
    }

    down.indicator: Component {
        Rectangle {
            x: control.mirrored ? control.width - width : 0
            height: control.height
            implicitWidth: 40
            implicitHeight: 40
            color: control.down.pressed ? control.palette.mid : control.palette.button
            border.color: enabled ? control.palette.text : control.palette.mid
            border.width: 0

            // A minus sign.
            Rectangle {
                x: (parent.width - width) / 2
                y: (parent.height - height) / 2
                width: parent.width / 3
                height: 2
                color: enabled ? control.palette.buttonText : control.palette.mid
            }
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 140
            color: enabled ? control.palette.base : control.palette.button
            border.color: control.palette.button
        }
    }
}
