// expect: layout-reflow at 8
import QtQuick
import QtQuick.Controls

Item {
    width: 320; height: 240
    // The text is laid out 640 px wide, so every line runs off to the side.
    Flickable {
        anchors.fill: parent
        contentWidth: notes.width; contentHeight: notes.height
        clip: true
        Label {
            id: notes
            width: 640
            wrapMode: Text.Wrap
            text: "Release notes. ".repeat(300)
        }
    }
}
