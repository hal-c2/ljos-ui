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
    readonly property int side: placement.side
    // Between it and its parent, in the window's pixels.
    property real gap: 4
    readonly property real anchorWidth: parent ? parent.width : 0
    readonly property real anchorHeight: parent ? parent.height : 0
    readonly property var placement: place()

    // Above or below it starts where its parent starts, and beside it, at
    // its parent's top.
    x: placement.x
    y: placement.y

    // A press on its parent does not close it, so a button that opens it
    // can close it again.
    closePolicy: T.Popup.CloseOnEscape | T.Popup.CloseOnPressOutsideParent
    // Escape closes a popup that has the keys, and a popover holding only
    // text would otherwise leave them outside it.
    focus: true

    // Where it goes, worked out where it is drawn: the window's content,
    // the top of the parent chain. A popup is drawn there unscaled, so its
    // parent's place, the room on each side and the gap are measured there,
    // through every scale and rotation of the parent's ancestors, and the
    // place found is given back in the parent's terms, as `x` and `y` are.
    // Each side's room leaves out the popup's margin on that side, which it
    // is kept inside of.
    function place() {
        const anchor = parent
        if (!anchor)
            return { side: edge, x: 0, y: 0 }
        // Mapping reads no property a binding sees, so each step's place
        // and transform is read here, which makes it a dependency.
        let top = anchor
        while (top.parent) {
            void [top.x, top.y, top.width, top.height, top.scale, top.rotation, top.transformOrigin]
            top = top.parent
        }
        const at = anchor.mapToItem(top, 0, 0, anchorWidth, anchorHeight)
        const margin = (m) => Math.max(m, 0)
        const above = at.y - margin(topMargin) - gap
        const below = top.height - margin(bottomMargin) - at.y - at.height - gap
        const before = at.x - margin(leftMargin) - gap
        const after = top.width - margin(rightMargin) - at.x - at.width - gap
        const side = edge === Qt.TopEdge ? (above < height && below > above ? Qt.BottomEdge : edge)
                   : edge === Qt.BottomEdge ? (below < height && above > below ? Qt.TopEdge : edge)
                   : edge === Qt.LeftEdge ? (before < width && after > before ? Qt.RightEdge : edge)
                   : (after < width && before > after ? Qt.LeftEdge : edge)
        const px = side === Qt.LeftEdge ? at.x - width - gap
                 : side === Qt.RightEdge ? at.x + at.width + gap
                 : mirrored ? at.x + at.width - width : at.x
        const py = side === Qt.TopEdge ? at.y - height - gap
                 : side === Qt.BottomEdge ? at.y + at.height + gap
                 : at.y
        const back = anchor.mapFromItem(top, px, py)
        return { side: side, x: back.x, y: back.y }
    }
}
