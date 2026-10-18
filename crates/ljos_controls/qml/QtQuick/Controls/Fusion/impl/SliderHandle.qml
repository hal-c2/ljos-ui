// The raised knob of Fusion's sliders and switches, a drawing of our own
// (CLEAN_ROOM.md): a white face shaded from the button's colour, tinted
// towards the highlight while the control has the keyboard, in an outline
// of its own that is the window's colour darker, or the highlight's while
// it has the keyboard. Unlike a button's it keeps its outline when not
// enabled. What qml6 shows (fusion-states.feature).
import QtQuick
import QtQuick.Templates as T
import QtQuick.Controls.Fusion

Rectangle {
    id: handle

    property T.Control control
    property bool visualFocus: false

    readonly property color button: control.palette.button
    readonly property color highlight: control.palette.highlight

    function shade(scale) {
        return Qt.rgba(button.r * scale, button.g * scale, button.b * scale, button.a)
    }

    implicitWidth: 13
    implicitHeight: 13
    radius: 2
    border.color: "transparent"
    gradient: Gradient {
        GradientStop { position: 0; color: Qt.lighter(handle.button, 1.07) }
        GradientStop {
            position: 1
            color: handle.visualFocus
                   ? Qt.tint(handle.shade(0.982), Qt.rgba(handle.highlight.r, handle.highlight.g, handle.highlight.b, 0.095))
                   : handle.shade(0.988)
        }
    }

    Rectangle {
        width: parent.width
        height: parent.height
        radius: parent.radius
        color: "transparent"
        border.color: handle.visualFocus ? Qt.darker(handle.highlight, 1.25)
                                         : Qt.darker(handle.control.palette.window, 1.4)

        // A faint light line just inside the outline.
        Rectangle {
            x: 1
            y: 1
            width: parent.width - 2
            height: parent.height - 2
            radius: parent.radius
            color: "transparent"
            border.color: Fusion.innerContrastLine
        }
    }
}
