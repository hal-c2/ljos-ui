// Ljós's own style (docs/DIALECT.md §4.16): a view that scrolls what is
// put in it, with the style's thin bars at its end and bottom edges. It
// has no ground of its own: it takes the one it lies on.
import QtQuick
import QtQuick.Templates as T

T.ScrollView {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    font.pixelSize: Theme.fontSize

    // Each bar shows while the other does: the pointer on one says the
    // view is being scrolled.
    ScrollBar.vertical: ScrollBar {
        id: down
        parent: control
        x: control.mirrored ? 0 : control.width - width
        y: control.topPadding
        height: control.availableHeight
        active: across.active
    }

    ScrollBar.horizontal: ScrollBar {
        id: across
        parent: control
        x: control.leftPadding
        y: control.height - height
        width: control.availableWidth
        active: down.active
    }
}
