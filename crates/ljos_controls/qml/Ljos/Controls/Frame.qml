// Ljós's own style (docs/DIALECT.md §4.16): a frame, a thin edge with the
// radius around a group, on whatever ground it lies on. A group that is
// raised from the ground as well is a Card.
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
            radius: Theme.radius
            color: "transparent"
            border.width: 1
            border.color: Theme.border
        }
    }
}
