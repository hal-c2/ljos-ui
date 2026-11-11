// Ljós's own style (docs/DIALECT.md §4.16): how a popup, a menu or a
// combo box's list comes in, a short fade. It goes at once: a fade out
// could swallow the next open.
import QtQuick

Transition {
    NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 120; easing.type: Easing.OutCubic }
}
