// The sunken field behind Fusion's text fields, a drawing of our own
// (CLEAN_ROOM.md): a faint shadow along the inside of its top, and while it
// has the keyboard an outline and a soft ring in the highlight.
import QtQuick
import QtQuick.Templates as T
import QtQuick.Controls.Fusion

Rectangle {
    id: panel

    property Item control
    readonly property color focusColor: Qt.darker(control.palette.highlight, 1.25)

    implicitWidth: 120
    implicitHeight: 24

    radius: 2
    color: control.palette.base
    border.color: control.activeFocus ? focusColor : Qt.darker(control.palette.button, 1.4)

    Rectangle {
        x: 1
        y: 1
        width: parent.width - 2
        height: parent.height - 2
        radius: 1.7
        color: "transparent"
        border.color: Qt.rgba(panel.focusColor.r, panel.focusColor.g, panel.focusColor.b, 40 / 255)
        visible: panel.control.activeFocus
    }

    Rectangle {
        x: 2
        y: 1
        width: parent.width - 4
        height: 1
        color: Fusion.topShadow
    }
}
