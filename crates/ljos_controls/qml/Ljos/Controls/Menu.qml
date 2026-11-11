// Ljós's own style (docs/DIALECT.md §4.16): a menu on the overlay surface,
// as wide as its widest row within bounds, that fades in, closes at once
// and shades as a popup does (Popup.qml, with the same parts).
import QtQuick
import QtQuick.Templates as T
import Ljos.Controls.impl

T.Menu {
    id: control

    // Whether any row has an icon, so the others keep its column: one of
    // the style's own, or the row's or its action's picture (`icon`
    // reads the action's where the row follows one).
    readonly property bool hasIcons: {
        for (let i = 0; i < count; i++) {
            const item = itemAt(i)
            if (!item)
                continue
            if (item.iconName !== undefined && item.iconName.length > 0)
                return true
            if (item.icon !== undefined && ("" + item.icon.source).length > 0)
                return true
        }
        return false
    }

    // The widest row's own width, so a long label or shortcut widens the
    // menu (within the bounds below) instead of being cut short.
    contentWidth: {
        let widest = 0
        for (let i = 0; i < count; i++) {
            const item = itemAt(i)
            if (item)
                widest = Math.max(widest, item.implicitWidth)
        }
        return widest
    }

    implicitWidth: Math.min(360, Math.max(220, contentWidth + leftPadding + rightPadding,
                                          implicitBackgroundWidth + leftInset + rightInset))
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    margins: 0
    padding: Theme.spacingSmall
    overlap: 1

    delegate: MenuItem {}

    enter: PopupEnter {}
    exit: null

    T.Overlay.modal: PopupShade {}
    T.Overlay.modeless: PopupShade { modal: false }

    contentItem: Component {
        ListView {
            implicitHeight: contentHeight
            model: control.contentModel
            interactive: contentHeight + control.topPadding + control.bottomPadding > control.height
            clip: true
            currentIndex: control.currentIndex
        }
    }

    background: Component {
        PopupSurface {
            implicitWidth: 220
            implicitHeight: Theme.controlHeight
        }
    }
}
