// Ljós's own style (docs/DIALECT.md §4.16): where a popover goes, beside
// the item it belongs to (its parent), whatever it looks like. The look is
// Popover.qml's, and Ljos.Controls.Basic's Popover is Basic's on the same
// placing.
import QtQuick
import QtQuick.Templates as T

T.Popup {
    id: control

    // The side of its parent it opens on: Qt.BottomEdge, Qt.TopEdge,
    // Qt.LeftEdge or Qt.RightEdge.
    property int edge: Qt.BottomEdge
    // The side it opens on: `edge`, or the other side where the window
    // has no room on that one and more on the other. A binding, so it is
    // settled before the popup opens and follows its parent as it moves.
    readonly property int side: placed()
    // Between it and its parent.
    property real gap: 4
    readonly property real anchorWidth: parent ? parent.width : 0
    readonly property real anchorHeight: parent ? parent.height : 0

    // Above or below it starts where its parent starts, and beside it, at
    // its parent's top.
    x: side === Qt.LeftEdge ? -width - gap
       : side === Qt.RightEdge ? anchorWidth + gap
       : mirrored ? anchorWidth - width : 0
    y: side === Qt.TopEdge ? -height - gap
       : side === Qt.BottomEdge ? anchorHeight + gap
       : 0

    // A press on its parent does not close it, so a button that opens it
    // can close it again.
    closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutsideParent

    function placed() {
        if (!parent)
            return edge
        // The room is the window's: the top of the parent chain is the
        // window's content. The parent's place in it is summed up the
        // chain, so each step is a dependency of the binding.
        let top = parent
        let atX = 0
        let atY = 0
        while (top.parent) {
            atX += top.x
            atY += top.y
            top = top.parent
        }
        const above = atY - gap
        const below = top.height - atY - anchorHeight - gap
        const before = atX - gap
        const after = top.width - atX - anchorWidth - gap
        if (edge === Qt.TopEdge)
            return above < height && below > above ? Qt.BottomEdge : edge
        if (edge === Qt.BottomEdge)
            return below < height && above > below ? Qt.TopEdge : edge
        if (edge === Qt.LeftEdge)
            return before < width && after > before ? Qt.RightEdge : edge
        return after < width && before > after ? Qt.LeftEdge : edge
    }
}
