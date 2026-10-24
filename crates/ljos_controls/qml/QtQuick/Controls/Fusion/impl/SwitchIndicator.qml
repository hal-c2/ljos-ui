// The track and handle of Fusion's switches, a drawing of our own
// (CLEAN_ROOM.md): a sunken groove that fills in the highlight from the
// start up to the handle, and a raised handle half the track's length.
import QtQuick
import QtQuick.Templates as T

SliderGroove {
    id: indicator

    implicitWidth: 40
    implicitHeight: 16

    // The fill fades in as the handle leaves the start, and stays in the
    // highlight while the switch is not enabled.
    fillOpacity: Math.min(1, handle.x / Math.max(1, width - handle.width) * 2)
    alwaysHighlighted: true
    progress: Math.max(handle.x + handle.width, handle.width) / Math.max(1, width)

    SliderHandle {
        id: handle
        x: Math.max(0, Math.min(indicator.width - width,
                                indicator.control.visualPosition * indicator.width - width / 2))
        width: 20
        height: 16
        control: indicator.control
        visualFocus: indicator.control.visualFocus
        // Held, not merely down (fusion-hover.feature).
        pressed: indicator.control.pressed
    }
}
