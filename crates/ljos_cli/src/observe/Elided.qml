// A line of text that may not fit: what is cut off is in its tool tip,
// for a pointer, and in its description, for a screen reader.
import QtQuick
import QtQuick.Controls

Text {
    id: line
    elide: Text.ElideRight
    maximumLineCount: 1
    // Saying what it is keeps it read by its words: a description alone
    // would make it a node with neither.
    Accessible.role: Accessible.StaticText
    Accessible.description: truncated ? text : ""
    ToolTip.text: text
    ToolTip.visible: truncated && hover.hovered
    ToolTip.delay: 500

    HoverHandler {
        id: hover
        enabled: line.truncated
    }
}
