// The sunken groove of Fusion's sliders and switches, a drawing of our own
// (CLEAN_ROOM.md), filled in the highlight over the share `offset` to
// `progress` of its length. It is Fusion's groove shade of the button
// colour, outlined in the window's (fusion-shades.feature). Its shading
// runs from top to bottom whichever way it lies, as qml6 shows
// (fusion-states.feature).
import QtQuick
import QtQuick.Controls.Fusion
import QtQuick.Templates as T

Rectangle {
    id: groove

    property T.Control control
    property real offset: 0
    property real progress: 0
    property bool vertical: false
    property real fillOpacity: 1
    // Whether the fill keeps the highlight when the control is not enabled:
    // the active one, as qml6 shows (palette-groups.feature).
    property bool alwaysHighlighted: false

    readonly property color button: control.palette.button
    readonly property color highlight: alwaysHighlighted ? control.palette.active.highlight
                                                         : control.palette.highlight
    readonly property bool lit: control.enabled || alwaysHighlighted

    function shade(c, scale) {
        return Qt.rgba(c.r * scale, c.g * scale, c.b * scale, c.a)
    }

    radius: 2
    border.color: Fusion.outline(control.palette)
    gradient: Gradient {
        GradientStop { position: 0; color: Qt.darker(Fusion.grooveColor(groove.control.palette), 1.1) }
        GradientStop { position: 1; color: Qt.lighter(Fusion.grooveColor(groove.control.palette), 1.1) }
    }

    Rectangle {
        x: groove.vertical ? 0 : groove.offset * groove.width
        y: groove.vertical ? (1 - groove.progress) * groove.height : 0
        width: groove.vertical ? groove.width : (groove.progress - groove.offset) * groove.width
        height: groove.vertical ? (groove.progress - groove.offset) * groove.height : groove.height
        radius: 2
        opacity: groove.fillOpacity
        visible: opacity > 0 && groove.progress > groove.offset
        border.width: groove.control.enabled ? 1 : groove.alwaysHighlighted ? 0 : 1
        border.color: groove.lit ? groove.shade(groove.highlight, 0.728) : groove.shade(groove.button, 0.438)
        gradient: Gradient {
                GradientStop {
                position: 0
                color: groove.lit ? groove.highlight : groove.shade(groove.button, 0.607)
            }
            GradientStop {
                position: 1
                color: groove.lit ? Qt.lighter(groove.highlight, 1.2) : groove.shade(groove.button, 0.728)
            }
        }
    }
}
