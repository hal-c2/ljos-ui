// The raised face Fusion's buttons share, a drawing of our own
// (CLEAN_ROOM.md). Its shades are Fusion's of the palette's button and
// highlight colours, and its outline is Fusion's of the window colour.
import QtQuick
import QtQuick.Templates as T
import QtQuick.Controls.Fusion

Rectangle {
    id: panel

    property T.Control control
    // Pressed in: flat, without the gradient.
    property bool pressed: false
    // Tinted towards the highlight, with an outline in it.
    property bool highlighted: false
    // The outline turns to the highlight while the control has the keyboard.
    property bool outlinesFocus: true

    // The shade the gradient runs from, as unpressed.
    readonly property color face: Fusion.buttonColor(control.palette, highlighted, false, false)

    radius: 2
    color: Fusion.buttonColor(control.palette, highlighted, pressed, false)
    gradient: pressed ? null : raised
    border.color: highlighted || outlinesFocus && control.visualFocus ? Fusion.highlightedOutline(control.palette)
                : Fusion.buttonOutline(control.palette, false, control.enabled)

    Gradient {
        id: raised
        GradientStop { position: 0; color: Fusion.gradientStart(panel.face) }
        GradientStop { position: 1; color: Fusion.gradientStop(panel.face) }
    }

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
