// Ljós's own style (docs/DIALECT.md §4.16): a card, a group of things on
// the surface, with a thin edge and the large radius. What is put in it is
// laid out in it, inside its padding.
import QtQuick
import QtQuick.Templates as T

T.Frame {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    padding: Theme.spacing + Theme.spacingSmall
    font.pixelSize: Theme.fontSize

    background: Component {
        Rectangle {
            radius: Theme.radiusLarge
            color: Theme.surface
            border.width: 1
            border.color: Theme.border
        }
    }
}
