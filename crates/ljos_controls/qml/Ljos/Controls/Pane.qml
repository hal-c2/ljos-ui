// Ljós's own style (docs/DIALECT.md §4.16): a pane, the canvas that
// controls are laid out on, with room around what is put in it. A pane
// that fills a window is the window's ground; a group raised from it is a
// Card or a Frame.
import QtQuick
import QtQuick.Templates as T

T.Pane {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    padding: Theme.spacing + Theme.spacingSmall
    font.pixelSize: Theme.fontSize

    background: Component {
        Rectangle {
            color: Theme.canvas
        }
    }
}
