// Ljós's own style (docs/DIALECT.md §4.16): a short note on the overlay
// surface.
import QtQuick
import QtQuick.Templates as T

T.ToolTip {
    id: control

    x: parent ? (parent.width - implicitWidth) / 2 : 0
    y: -implicitHeight - 3

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    margins: 6
    padding: Theme.spacingSmall
    horizontalPadding: Theme.spacing
    font.pixelSize: Theme.fontSizeSmall


    contentItem: Component {
        Text {
            text: control.text
            font: control.font
            wrapMode: Text.Wrap
            color: Theme.text
        }
    }

    background: Component {
        Rectangle {
            radius: Theme.radiusSmall
            color: Theme.surfaceOverlay
            border.width: 1
            border.color: Theme.border
        }
    }
}
