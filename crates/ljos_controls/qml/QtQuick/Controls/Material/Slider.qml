// The Material style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Material's own metrics and attached palette.
import QtQuick
import QtQuick.Controls.Material
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
            implicitWidth: 20
            implicitHeight: 20
            radius: width / 2
            color: control.Material.accent
        }
    }

    background: Component {
        Rectangle {
            x: control.leftPadding + (control.horizontal ? 0 : (control.availableWidth - width) / 2)
            y: control.topPadding + (control.horizontal ? (control.availableHeight - height) / 2 : 0)
            implicitWidth: control.horizontal ? 200 : 48
            implicitHeight: control.horizontal ? 48 : 200
            width: control.horizontal ? control.availableWidth : 4
            height: control.horizontal ? 4 : control.availableHeight
            radius: 2
            color: control.Material.dividerColor
            scale: control.horizontal && control.mirrored ? -1 : 1

            Rectangle {
                y: control.horizontal ? 0 : control.visualPosition * parent.height
                width: control.horizontal ? control.position * parent.width : 4
                height: control.horizontal ? 4 : (1.0 - control.visualPosition) * parent.height
                radius: 2
                color: control.Material.accent
            }
        }
    }
}
