// Ljós's own style (docs/DIALECT.md §4.16): a bar of tools on the
// surface, an edge on the side it meets the page: under a header, over a
// footer.
import QtQuick
import QtQuick.Templates as T

T.ToolBar {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    padding: Theme.spacingSmall
    leftPadding: Theme.spacing
    rightPadding: Theme.spacing
    spacing: Theme.spacingSmall
    font.pixelSize: Theme.fontSize

    background: Component {
        Rectangle {
            implicitHeight: Theme.controlHeight + Theme.spacing
            color: Theme.surface

            Rectangle {
                objectName: "edge"
                width: parent.width
                height: 1
                y: control.position === T.ToolBar.Footer ? 0 : parent.height - 1
                color: Theme.border
            }
        }
    }
}
