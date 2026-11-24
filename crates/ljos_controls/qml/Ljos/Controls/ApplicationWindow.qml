// Ljós's own style (docs/DIALECT.md §4.16): a window whose ground is the
// theme's canvas, and whose controls take the theme's type.
import QtQuick
import QtQuick.Templates as T

T.ApplicationWindow {
    id: window

    color: Theme.canvas
    font.pixelSize: Theme.fontSize
}
