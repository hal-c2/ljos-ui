// expect: color-accent-weight at 5
import QtQuick
import QtQuick.Controls

Rectangle {
    width: 360; height: 240
    // The whole page is in the accent.
    color: palette.highlight
    Label {
        anchors.centerIn: parent
        text: "Welcome"
        color: palette.highlightedText
    }
}
