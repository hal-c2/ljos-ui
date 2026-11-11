// Ljós's own style (docs/DIALECT.md §4.16): what a popup, a menu or a
// combo box's list is drawn on, the overlay surface in a frame, lifted
// off the window by a soft shadow below it.
import QtQuick
import QtQuick.Effects
import Ljos.Controls

Item {
    RectangularShadow {
        x: frame.x
        y: frame.y
        width: frame.width
        height: frame.height
        radius: frame.radius
        offset.y: 8
        blur: 24
        color: Qt.rgba(0, 0, 0, Theme.dark ? 0.5 : 0.12)
    }

    Rectangle {
        id: frame
        width: parent.width
        height: parent.height
        radius: Theme.radiusLarge
        color: Theme.surfaceOverlay
        border.width: 1
        border.color: Theme.border
    }
}
