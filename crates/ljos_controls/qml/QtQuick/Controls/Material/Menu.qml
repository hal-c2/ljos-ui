// The Material style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Material's own metrics and attached palette.
// Basic's menu, with the shade of this style.
import QtQuick
import QtQuick.Controls.Basic as Basic
import QtQuick.Controls.Material
import QtQuick.Templates as T

Basic.Menu {
    id: control

    // What is under the popup while it dims it: a dark shade in the light
    // theme and a light one in the dark, modal or not. It fades in as the
    // popup opens and out as it closes.
    T.Overlay.modal: Rectangle {
        color: control.Material.theme === Material.Dark ? "#99fafafa" : "#99303030"
        Behavior on opacity { NumberAnimation { duration: 150 } }
    }
    T.Overlay.modeless: Rectangle {
        color: control.Material.theme === Material.Dark ? "#99fafafa" : "#99303030"
        Behavior on opacity { NumberAnimation { duration: 150 } }
    }
}
