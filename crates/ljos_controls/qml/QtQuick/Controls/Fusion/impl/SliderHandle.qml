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

    // Fusion's shade of a button, or a highlighted one's with the keyboard.
    readonly property color face: Fusion.buttonColor(control.palette, visualFocus, false, false)

    implicitWidth: 13
    implicitHeight: 13
    radius: 2
    border.color: "transparent"
    gradient: Gradient {
        GradientStop { position: 0; color: Fusion.gradientStart(handle.face) }
        GradientStop { position: 1; color: Fusion.gradientStop(handle.face) }
    }

    Rectangle {
        width: parent.width
        height: parent.height
        radius: parent.radius
        color: "transparent"
        border.color: handle.visualFocus ? Fusion.highlightedOutline(handle.control.palette)
                                         : Fusion.outline(handle.control.palette)

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
