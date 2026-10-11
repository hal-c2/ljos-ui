// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.ScrollView {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

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
