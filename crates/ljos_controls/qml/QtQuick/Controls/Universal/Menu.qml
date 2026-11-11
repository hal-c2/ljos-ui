// The Universal style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Universal's own metrics and attached palette.
// Basic's menu, with the shade of this style.
import QtQuick
import QtQuick.Controls.Basic as Basic
import QtQuick.Controls.Universal
import QtQuick.Templates as T

Basic.Menu {
    id: control

    // What is under the popup while it dims it: the theme's base low
    // colour, modal or not.
    T.Overlay.modal: Rectangle {
        color: control.Universal.baseLowColor
    }
    T.Overlay.modeless: Rectangle {
        color: control.Universal.baseLowColor
    }
}
