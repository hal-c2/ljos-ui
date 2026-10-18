// The raised knob of Fusion's sliders and switches, a drawing of our own
// (CLEAN_ROOM.md): a button face, tinted towards the highlight and outlined
// in it while the control has the keyboard. Unlike a button's it keeps its
// outline when not enabled.
import QtQuick

ButtonPanel {
    property bool visualFocus: false

    implicitWidth: 13
    implicitHeight: 13
    highlighted: visualFocus
    border.color: visualFocus ? Qt.darker(highlight, 1.25) : Qt.darker(button, 1.4)
}
