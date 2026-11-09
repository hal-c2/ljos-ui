// Ljós's own style (docs/DIALECT.md §4.16): a groove in the field's edge
// colour, filled with the accent up to the value, and a 16-pixel handle
// with a ring where the keyboard is.
import QtQuick
import QtQuick.Templates as T

T.Slider {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitHandleWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitHandleHeight + topPadding + bottomPadding)

    // Room for the ring around the handle, and a 24-pixel target.
    padding: 4
    hoverEnabled: true
    opacity: enabled ? 1 : 0.64

    handle: Component {
        Rectangle {
            x: control.leftPadding + (control.horizontal
                                      ? control.visualPosition * (control.availableWidth - width)
                                      : (control.availableWidth - width) / 2)
            y: control.topPadding + (control.horizontal
                                     ? (control.availableHeight - height) / 2
                                     : control.visualPosition * (control.availableHeight - height))
            implicitWidth: 16
            implicitHeight: 16
            radius: width / 2
            color: control.pressed ? Theme.surfaceOverlay : Theme.canvas
            border.width: control.visualFocus ? 2 : 1
            border.color: control.visualFocus ? Theme.focus : Theme.input
        }
    }

    background: Component {
        Rectangle {
            x: control.leftPadding + (control.horizontal ? 0 : (control.availableWidth - width) / 2)
            y: control.topPadding + (control.horizontal ? (control.availableHeight - height) / 2 : 0)
            implicitWidth: control.horizontal ? 160 : 4
            implicitHeight: control.horizontal ? 4 : 160
            width: control.horizontal ? control.availableWidth : implicitWidth
            height: control.horizontal ? implicitHeight : control.availableHeight
            radius: 2
            color: Theme.input
            scale: control.horizontal && control.mirrored ? -1 : 1

            Rectangle {
                y: control.horizontal ? 0 : control.visualPosition * parent.height
                width: control.horizontal ? control.position * parent.width : 4
                height: control.horizontal ? 4 : control.position * parent.height
                radius: 2
                color: Theme.accent
            }
        }
    }
}
