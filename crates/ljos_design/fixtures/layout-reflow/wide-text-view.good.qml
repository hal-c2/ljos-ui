import QtQuick
import QtQuick.Controls

Item {
    width: 320; height: 240
    Flickable {
        anchors.fill: parent
        contentWidth: width; contentHeight: notes.height
        clip: true
        Label {
            id: notes
            width: parent.width
            wrapMode: Text.Wrap
            text: "Release notes. ".repeat(300)
        }
    }
}
