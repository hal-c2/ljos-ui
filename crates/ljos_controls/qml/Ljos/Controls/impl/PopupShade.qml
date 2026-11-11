// Ljós's own style (docs/DIALECT.md §4.16): what is under a popup while
// it dims it, a grey half as strong behind a modal popup as it is, and an
// eighth behind one that only dims.
import QtQuick

Rectangle {
    property bool modal: true

    color: Qt.alpha("#28282a", modal ? 0.5 : 0.12)
}
