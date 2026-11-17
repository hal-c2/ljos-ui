// Ljós's own style (docs/DIALECT.md §4.16): a popover, a popup beside the
// item it belongs to (its parent) on the overlay surface, below it unless
// `edge` says another side, and on the other side where the window has no
// room (impl/PopoverBase.qml). It fades in and closes at once, as a popup
// does, and shades nothing unless it is made modal.
import QtQuick
import QtQuick.Templates as T
import Ljos.Controls.impl

PopoverBase {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    gap: Theme.spacingSmall
    margins: Theme.spacing
    padding: Theme.spacingSmall
    font.pixelSize: Theme.fontSize

    enter: PopupEnter {}
    exit: null

    T.Overlay.modal: PopupShade {}
    T.Overlay.modeless: PopupShade { modal: false }

    background: Component {
        PopupSurface {}
    }
}
