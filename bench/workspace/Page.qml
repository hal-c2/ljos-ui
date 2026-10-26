// A page of the workspace's side panel: a long list of entries, all made.
import QtQuick

Flickable {
    id: page
    property int seed
    contentHeight: column.height
    clip: true
    Column {
        id: column
        width: page.width
        Repeater {
            model: 400
            Item {
                required property int index
                width: column.width
                height: 28
                Rectangle {
                    x: 8
                    width: 4
                    height: 20
                    anchors.verticalCenter: parent.verticalCenter
                    color: (index + page.seed) % 3 ? "#9bb" : "#b99"
                }
                Text {
                    x: 20
                    width: parent.width - 28
                    anchors.verticalCenter: parent.verticalCenter
                    text: "Entry " + (page.seed * 1000 + index)
                    elide: Text.ElideRight
                }
            }
        }
    }
}
