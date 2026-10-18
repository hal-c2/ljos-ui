// The raised face Fusion's buttons share, a drawing of our own
// (CLEAN_ROOM.md). Its shades are fitted to what qml6 shows for the default
// palette and follow the palette's button and highlight colours.
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

    readonly property color button: control.palette.button
    readonly property color highlight: control.palette.highlight

    function shade(scale) {
        return Qt.rgba(button.r * scale, button.g * scale, button.b * scale, button.a)
    }
    function tinted(scale) {
        return Qt.tint(shade(scale), Qt.rgba(highlight.r, highlight.g, highlight.b, 0.095))
    }

    radius: 2
    color: pressed ? shade(0.883) : highlighted ? tinted(0.962) : Qt.darker(button, 1.03)
    gradient: pressed ? null : raised
    border.color: highlighted || outlinesFocus && control.visualFocus ? Qt.darker(highlight, 1.25)
                : control.enabled ? Qt.darker(button, 1.4)
                : Qt.darker(button, 1.22)

    Gradient {
        id: raised
        GradientStop { position: 0; color: Qt.lighter(panel.button, 1.07) }
        GradientStop { position: 1; color: panel.highlighted ? panel.tinted(0.982) : panel.shade(0.988) }
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
