// Ljós's own style (docs/DIALECT.md §4.16): a field with the number in the
// middle and a minus and a plus at its ends.
import QtQuick
import QtQuick.Templates as T

T.SpinBox {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentItem.implicitWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             up.implicitIndicatorHeight, down.implicitIndicatorHeight)

    padding: 0
    leftPadding: control.mirrored ? (up.indicator ? up.indicator.width : 0)
                                  : (down.indicator ? down.indicator.width : 0)
    rightPadding: control.mirrored ? (down.indicator ? down.indicator.width : 0)
                                   : (up.indicator ? up.indicator.width : 0)
    hoverEnabled: true
    font.pixelSize: Theme.fontSize
    opacity: enabled ? 1 : 0.64

    // Above the buttons, so that a press on the number is the number's.
    contentItem: Component {
        TextInput {
            z: 2
            text: control.displayText
            font: control.font
            color: Theme.text
            selectionColor: Theme.accent
            selectedTextColor: Theme.accentForeground
            horizontalAlignment: Qt.AlignHCenter
            verticalAlignment: Qt.AlignVCenter
            readOnly: !control.editable
            validator: control.validator
            inputMethodHints: control.inputMethodHints
        }
    }

    up.indicator: Component {
        Rectangle {
            x: control.mirrored ? 0 : control.width - width
            y: 0
            implicitWidth: Theme.controlHeight
            implicitHeight: Theme.controlHeight
            height: control.height
            radius: Theme.radius
            color: control.up.pressed || control.up.hovered ? Theme.accentSurface : "transparent"

            Icon {
                x: (parent.width - width) / 2
                y: (parent.height - height) / 2
                name: "plus"
                size: 14
                color: control.value < control.to ? Theme.text : Theme.iconMuted
            }
        }
    }

    down.indicator: Component {
        Rectangle {
            x: control.mirrored ? control.width - width : 0
            y: 0
            implicitWidth: Theme.controlHeight
            implicitHeight: Theme.controlHeight
            height: control.height
            radius: Theme.radius
            color: control.down.pressed || control.down.hovered ? Theme.accentSurface : "transparent"

            Icon {
                x: (parent.width - width) / 2
                y: (parent.height - height) / 2
                name: "minus"
                size: 14
                color: control.value > control.from ? Theme.text : Theme.iconMuted
            }
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 120
            implicitHeight: Theme.controlHeight
            radius: Theme.radius
            color: Theme.surfaceRaised
            border.width: control.activeFocus ? 2 : 1
            border.color: control.activeFocus ? Theme.focus : Theme.input
        }
    }
}
