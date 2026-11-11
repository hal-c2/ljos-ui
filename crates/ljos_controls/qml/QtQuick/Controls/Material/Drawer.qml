// The Material style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Material's own metrics and attached palette.
// Basic's drawer, with the shade of this style.
import QtQuick
import QtQuick.Controls.Basic as Basic
import QtQuick.Controls.Material
import QtQuick.Templates as T

Basic.Drawer {
    id: control

    // What is under the popup while it dims it: a dark shade in the light
    // theme and a light one in the dark, modal or not. It comes in and
    // goes with the drawer.
    T.Overlay.modal: Rectangle {
        color: control.Material.theme === Material.Dark ? "#99fafafa" : "#99303030"
    }
    T.Overlay.modeless: Rectangle {
        color: control.Material.theme === Material.Dark ? "#99fafafa" : "#99303030"
    }
}
