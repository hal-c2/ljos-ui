// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.RangeSlider {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            first.implicitHandleWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             first.implicitHandleHeight + topPadding + bottomPadding)

    padding: 6

    // The handles take the keyboard in turn. To an assistive technology
    // each is a slider of its own (the template names it and steps it);
    // the style says where it is, in the properties `Accessible` reads of
    // a slider.
    Accessible.focusable: true

    // Two round handles, each travelling the track inside the padding.
    first.handle: Component {
        Rectangle {
            x: control.leftPadding + (control.horizontal
                                      ? control.first.visualPosition * (control.availableWidth - width)
                                      : (control.availableWidth - width) / 2)
            y: control.topPadding + (control.horizontal
                                     ? (control.availableHeight - height) / 2
                                     : control.first.visualPosition * (control.availableHeight - height))
            readonly property real value: control.first.value
            readonly property real minimumValue: Math.min(control.from, control.to)
            readonly property real maximumValue: Math.max(control.from, control.to)
            // No step size is the step increase() takes without one.
            readonly property real stepSize: control.stepSize > 0 ? control.stepSize : 0.1
            implicitWidth: 28
            implicitHeight: 28
            radius: width / 2
            color: control.first.pressed ? control.palette.light : control.palette.window
            border.width: activeFocus && control.visualFocus ? 2 : 1
            border.color: activeFocus && control.visualFocus
                          ? control.palette.highlight
                          : control.enabled ? control.palette.mid : control.palette.midlight
        }
    }

    second.handle: Component {
        Rectangle {
            x: control.leftPadding + (control.horizontal
                                      ? control.second.visualPosition * (control.availableWidth - width)
                                      : (control.availableWidth - width) / 2)
            y: control.topPadding + (control.horizontal
                                     ? (control.availableHeight - height) / 2
                                     : control.second.visualPosition * (control.availableHeight - height))
            readonly property real value: control.second.value
            readonly property real minimumValue: Math.min(control.from, control.to)
            readonly property real maximumValue: Math.max(control.from, control.to)
            // No step size is the step increase() takes without one.
            readonly property real stepSize: control.stepSize > 0 ? control.stepSize : 0.1
            implicitWidth: 28
            implicitHeight: 28
            radius: width / 2
            color: control.second.pressed ? control.palette.light : control.palette.window
            border.width: activeFocus && control.visualFocus ? 2 : 1
            border.color: activeFocus && control.visualFocus
                          ? control.palette.highlight
                          : control.enabled ? control.palette.mid : control.palette.midlight
        }
    }

    // A thin track along the middle, filled between the two handles.
    background: Component {
        Rectangle {
            x: control.leftPadding + (control.horizontal ? 0 : (control.availableWidth - width) / 2)
            y: control.topPadding + (control.horizontal ? (control.availableHeight - height) / 2 : 0)
            implicitWidth: control.horizontal ? 200 : 6
            implicitHeight: control.horizontal ? 6 : 200
            width: control.horizontal ? control.availableWidth : implicitWidth
            height: control.horizontal ? implicitHeight : control.availableHeight
            radius: 3
            color: control.palette.midlight
            scale: control.horizontal && control.mirrored ? -1 : 1

            Rectangle {
                x: control.horizontal ? control.first.position * parent.width : 0
                y: control.horizontal ? 0 : control.second.visualPosition * parent.height
                width: control.horizontal
                       ? (control.second.position - control.first.position) * parent.width
                       : 6
                height: control.horizontal
                        ? 6
                        : (control.second.position - control.first.position) * parent.height
                radius: 3
                color: control.palette.dark
            }
        }
    }
}
