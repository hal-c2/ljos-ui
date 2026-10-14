// The Fusion style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Fusion's own metrics and palette.
import QtQuick
import QtQuick.Templates as T

T.Slider {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitHandleWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitHandleHeight + topPadding + bottomPadding)

    padding: 0

    handle: Component {
        Rectangle {
            x: control.leftPadding + (control.horizontal ? control.visualPosition * (control.availableWidth - width) : (control.availableWidth - width) / 2)
            y: control.topPadding + (control.horizontal ? (control.availableHeight - height) / 2 : control.visualPosition * (control.availableHeight - height))
            implicitWidth: 13
            implicitHeight: 13
            radius: 3
            color: control.palette.button
            border.color: control.palette.mid
        }
    }

    background: Component {
        Rectangle {
            x: control.leftPadding + (control.horizontal ? 0 : (control.availableWidth - width) / 2)
            y: control.topPadding + (control.horizontal ? (control.availableHeight - height) / 2 : 0)
            implicitWidth: control.horizontal ? 160 : 5
            implicitHeight: control.horizontal ? 5 : 160
            width: control.horizontal ? control.availableWidth : implicitWidth
            height: control.horizontal ? implicitHeight : control.availableHeight
            radius: 2
            color: control.palette.dark
            scale: control.horizontal && control.mirrored ? -1 : 1

            Rectangle {
                y: control.horizontal ? 0 : control.visualPosition * parent.height
                width: control.horizontal ? control.position * parent.width : 5
                height: control.horizontal ? 5 : (1.0 - control.visualPosition) * parent.height
                radius: 2
                color: control.palette.highlight
            }
        }
    }
}
