// The Universal style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Universal's own metrics and attached palette.
import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Templates as T

T.Slider {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitHandleWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitHandleHeight + topPadding + bottomPadding)

    padding: 6

    handle: Component {
        Rectangle {
            x: control.leftPadding + (control.horizontal ? control.visualPosition * (control.availableWidth - width) : (control.availableWidth - width) / 2)
            y: control.topPadding + (control.horizontal ? (control.availableHeight - height) / 2 : control.visualPosition * (control.availableHeight - height))
            implicitWidth: 24
            implicitHeight: 24
            radius: 12
            color: control.Universal.accent
        }
    }

    background: Component {
        Rectangle {
            x: control.leftPadding + (control.horizontal ? 0 : (control.availableWidth - width) / 2)
            y: control.topPadding + (control.horizontal ? (control.availableHeight - height) / 2 : 0)
            implicitWidth: control.horizontal ? 200 : 18
            implicitHeight: control.horizontal ? 18 : 200
            width: control.horizontal ? control.availableWidth : 2
            height: control.horizontal ? 2 : control.availableHeight
            color: control.Universal.baseLowColor
            scale: control.horizontal && control.mirrored ? -1 : 1

            Rectangle {
                y: control.horizontal ? 0 : control.visualPosition * parent.height
                width: control.horizontal ? control.position * parent.width : 2
                height: control.horizontal ? 2 : (1.0 - control.visualPosition) * parent.height
                color: control.Universal.accent
            }
        }
    }
}
