// Ljós's own style (docs/DIALECT.md §4.16): a control of an app's own,
// sized by what it holds and taking the theme's type.
import QtQuick
import QtQuick.Templates as T

T.Control {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    font.pixelSize: Theme.fontSize
}
