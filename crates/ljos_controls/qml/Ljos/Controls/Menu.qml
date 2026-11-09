// Ljós's own style (docs/DIALECT.md §4.16): a menu on the overlay surface,
// as wide as its widest row within bounds, that fades in and closes at once.
import QtQuick
import QtQuick.Effects
import QtQuick.Templates as T

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

    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 120; easing.type: Easing.OutCubic }
    }
    // Closing is at once: a fade could swallow the next open.
    exit: null

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
        Item {
            implicitWidth: 220
            implicitHeight: Theme.controlHeight

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
    }
}
