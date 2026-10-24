// The Fusion style of Ljós (CLEAN_ROOM.md), drawn with its parts in
// QtQuick.Controls.Fusion.impl: drawings of our own, fitted to what qml6 shows.
import QtQuick
import QtQuick.Controls.Fusion.impl
import QtQuick.Templates as T

T.RangeSlider {
    // Not `control`: inside a part's component that names the part's
    // own property, which Fusion's parts have.
    id: host

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            first.implicitHandleWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             first.implicitHandleHeight + topPadding + bottomPadding)

    padding: 0

    // The handles take the keyboard in turn. To an assistive technology
    // each is a slider of its own (the template names it and steps it);
    // the style says where it is, in the properties `Accessible` reads of
    // a slider.
    Accessible.focusable: true

    // On whole pixels, as qml6 places them; turned with a vertical slider
    // so that their shading runs across the groove.
    first.handle: Component {
        SliderHandle {
            x: Math.round(host.leftPadding + (host.horizontal ? host.first.visualPosition * (host.availableWidth - width) : (host.availableWidth - width) / 2))
            y: Math.round(host.topPadding + (host.horizontal ? (host.availableHeight - height) / 2 : host.first.visualPosition * (host.availableHeight - height)))
            rotation: host.horizontal ? 0 : -90
            control: host
            visualFocus: activeFocus && host.visualFocus
            pressed: host.first.pressed
            readonly property real value: host.first.value
            readonly property real minimumValue: Math.min(host.from, host.to)
            readonly property real maximumValue: Math.max(host.from, host.to)
            // No step size is the step increase() takes without one.
            readonly property real stepSize: host.stepSize > 0 ? host.stepSize : 0.1
        }
    }

    second.handle: Component {
        SliderHandle {
            x: Math.round(host.leftPadding + (host.horizontal ? host.second.visualPosition * (host.availableWidth - width) : (host.availableWidth - width) / 2))
            y: Math.round(host.topPadding + (host.horizontal ? (host.availableHeight - height) / 2 : host.second.visualPosition * (host.availableHeight - height)))
            rotation: host.horizontal ? 0 : -90
            control: host
            visualFocus: activeFocus && host.visualFocus
            pressed: host.second.pressed
            readonly property real value: host.second.value
            readonly property real minimumValue: Math.min(host.from, host.to)
            readonly property real maximumValue: Math.max(host.from, host.to)
            // No step size is the step increase() takes without one.
            readonly property real stepSize: host.stepSize > 0 ? host.stepSize : 0.1
        }
    }

    // Filled in the highlight between the two handles.
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
            offset: host.first.position
            progress: host.second.position
            scale: host.horizontal && host.mirrored ? -1 : 1
        }
    }
}
