// Ljós's own style (docs/DIALECT.md §4.16): a page, the canvas with a
// header and a footer laid out above and below what is put in it.
import QtQuick
import QtQuick.Templates as T

T.Page {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding,
                            implicitHeaderWidth,
                            implicitFooterWidth)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding
                             + (implicitHeaderHeight > 0 ? implicitHeaderHeight + spacing : 0)
                             + (implicitFooterHeight > 0 ? implicitFooterHeight + spacing : 0))

    font.pixelSize: Theme.fontSize

    background: Component {
        Rectangle {
            color: Theme.canvas
        }
    }
}
