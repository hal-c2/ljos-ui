// The Fusion style of Ljós (CLEAN_ROOM.md), drawn with its parts in
// QtQuick.Controls.Fusion.impl: drawings of our own, fitted to what qml6 shows.
import QtQuick
import QtQuick.Controls.Fusion.impl
import QtQuick.Templates as T

T.Slider {
    // Not `control`: inside a part's component that names the part's
    // own property, which Fusion's parts have.
    id: host

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitHandleWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitHandleHeight + topPadding + bottomPadding)

    padding: 0

    // On whole pixels, as qml6 places it; turned with a vertical slider so
    // that its shading runs across the groove.
    handle: Component {
        SliderHandle {
            x: Math.round(host.leftPadding + (host.horizontal ? host.visualPosition * (host.availableWidth - width) : (host.availableWidth - width) / 2))
            y: Math.round(host.topPadding + (host.horizontal ? (host.availableHeight - height) / 2 : host.visualPosition * (host.availableHeight - height)))
            rotation: host.horizontal ? 0 : -90
            control: host
            visualFocus: host.visualFocus
        }
    }

    background: Component {
        SliderGroove {
            x: host.leftPadding + (host.horizontal ? 0 : (host.availableWidth - width) / 2)
            y: host.topPadding + (host.horizontal ? (host.availableHeight - height) / 2 : 0)
            implicitWidth: host.horizontal ? 160 : 5
            implicitHeight: host.horizontal ? 5 : 160
            width: host.horizontal ? host.availableWidth : implicitWidth
            height: host.horizontal ? implicitHeight : host.availableHeight
            control: host
            vertical: host.vertical
            progress: host.position
            scale: host.horizontal && host.mirrored ? -1 : 1
        }
    }
}
