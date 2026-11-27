// A word with a mark before it in the colour of what it tells of. The
// semantic colours are made to be seen beside text, not to be read as
// text, so the word stays in a colour it can be read in.
import QtQuick
import Ljos.Design

Elided {
    id: marked
    property color tone: "transparent"
    leftPadding: tone.a > 0 ? Spacing.sm : 0
    color: Colors.text
    Rectangle {
        visible: marked.tone.a > 0
        anchors.verticalCenter: parent.verticalCenter
        width: 6; height: 6; radius: 3
        color: marked.tone
    }
}
