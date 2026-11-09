// Ljós's own style (docs/DIALECT.md §4.16): text in the theme's text
// colour and size.
import QtQuick
import QtQuick.Templates as T

T.Label {
    color: Theme.text
    linkColor: Theme.link
    font.pixelSize: Theme.fontSize
}
