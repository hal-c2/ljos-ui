// Ljós's own style (docs/DIALECT.md §4.16): a dialog is a raised surface
// over a shade, as a popup is, with its title in the heavier weight and
// its buttons at the end of its foot, the one that accepts it filled.
import QtQuick
import QtQuick.Templates as T
import Ljos.Controls.impl

T.Dialog {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding,
                            implicitHeaderWidth,
                            implicitFooterWidth)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding
                             + (implicitHeaderHeight > 0 ? implicitHeaderHeight + spacing : 0)
                             + (implicitFooterHeight > 0 ? implicitFooterHeight + spacing : 0))

    padding: Theme.spacing * 2
    topPadding: Theme.spacing
    font.pixelSize: Theme.fontSize

    enter: PopupEnter {}
    exit: null

    T.Overlay.modal: PopupShade {}
    T.Overlay.modeless: PopupShade { modal: false }

    background: Component {
        PopupSurface {}
    }

    header: Component {
        Item {
            property alias text: label.text
            property alias font: label.font

            visible: control.title.length > 0
            implicitWidth: label.implicitWidth + Theme.spacing * 4
            implicitHeight: label.implicitHeight + Theme.spacing * 2

            Text {
                id: label
                objectName: "title"
                x: Theme.spacing * 2
                y: Theme.spacing * 2
                width: parent.width - Theme.spacing * 4
                text: control.title
                font.pixelSize: Theme.fontSize + 2
                font.weight: Font.DemiBold
                color: Theme.text
                elide: Text.ElideRight
                horizontalAlignment: control.mirrored ? Text.AlignRight : Text.AlignLeft
            }
        }
    }

    footer: Component {
        DialogButtonBox {
            visible: count > 0
        }
    }
}
