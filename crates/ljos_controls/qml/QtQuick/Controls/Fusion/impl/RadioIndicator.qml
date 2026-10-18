// The ring of Fusion's radio buttons, a drawing of our own (CLEAN_ROOM.md):
// a faint shadow inside its top, and a dark dot when chosen. Its shades are
// fitted to what qml6 shows for the default palette.
import QtQuick
import QtQuick.Templates as T

Rectangle {
    id: indicator

    property T.AbstractButton control

    implicitWidth: 14
    implicitHeight: 14

    radius: width / 2
    color: control.down ? Qt.darker(control.palette.light, 1.18) : control.palette.light
    border.color: control.visualFocus ? Qt.darker(control.palette.highlight, 1.25)
                                      : Qt.darker(control.palette.window, 1.5)

    Rectangle {
        y: 1
        width: parent.width
        height: parent.height - 1
        visible: indicator.control.enabled && !indicator.control.down
        radius: width / 2
        color: "transparent"
        border.color: Qt.rgba(0, 0, 0, 18 / 255)
    }

    Rectangle {
        width: parent.width / 2.32
        height: width
        x: (parent.width - width) / 2
        y: (parent.height - height) / 2
        radius: width / 2
        // Not enabled, the dot is a grey a little darker than the text's,
        // as on qml6.
        readonly property color ink: indicator.control.enabled
            ? indicator.control.palette.text
            : Qt.rgba(indicator.control.palette.mid.r * 0.86, indicator.control.palette.mid.g * 0.86, indicator.control.palette.mid.b * 0.86, 1)
        color: Qt.rgba(ink.r, ink.g, ink.b, 180 / 255)
        border.color: Qt.rgba(ink.r, ink.g, ink.b, 200 / 255)
        visible: indicator.control.checked
    }
}
