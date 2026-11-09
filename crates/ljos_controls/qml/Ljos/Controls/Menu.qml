// Ljós's own style (docs/DIALECT.md §4.16): a menu on the overlay surface,
// as wide as its widest row within bounds, that fades in and closes at once.
import QtQuick
import QtQuick.Effects
import QtQuick.Templates as T

T.Menu {
    id: control

    // Whether any row has an icon, so the others keep its column.
    readonly property bool hasIcons: {
        for (let i = 0; i < count; i++) {
            const item = itemAt(i)
            if (item && item.iconName !== undefined && item.iconName.length > 0)
                return true
        }
        return false
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
