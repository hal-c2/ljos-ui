// Ljós's own style (docs/DIALECT.md §4.16): a popup on the overlay surface
// that fades in, closes at once, and shades what is under it while it
// dims it. Menu.qml draws with the same parts (Ljos.Controls.impl).
import QtQuick
import QtQuick.Templates as T
import Ljos.Controls.impl

T.Popup {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    padding: Theme.spacingSmall

    enter: PopupEnter {}
    exit: null

    T.Overlay.modal: PopupShade {}
    T.Overlay.modeless: PopupShade { modal: false }

    background: Component {
        PopupSurface {}
    }
}
