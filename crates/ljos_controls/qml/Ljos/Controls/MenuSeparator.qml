// Ljós's own style (docs/DIALECT.md §4.16): a faint line between groups
// of a menu.
import QtQuick
import QtQuick.Templates as T

T.MenuSeparator {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 0
    topPadding: Theme.spacingSmall
    bottomPadding: Theme.spacingSmall

    contentItem: Component {
        Rectangle {
            implicitWidth: 188
            implicitHeight: 1
            color: Theme.border
        }
    }
}
