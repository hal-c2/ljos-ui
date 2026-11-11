// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports.
// Basic's drawer, with the shade of this style.
import QtQuick
import QtQuick.Controls.Basic as Basic
import QtQuick.Controls.Fusion
import QtQuick.Templates as T

Basic.Drawer {
    id: control

    // What is under the popup while it dims it: the same faint black,
    // modal or not.
    T.Overlay.modal: Rectangle {
        color: Fusion.topShadow
    }
    T.Overlay.modeless: Rectangle {
        color: Fusion.topShadow
    }
}
