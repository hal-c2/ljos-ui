import QtQuick
import QtQuick.Controls
import Example

// A Repeater over rows a native property holds (a list of records made in
// Rust), each delegate requiring its `modelData` and `index`.
Item {
    id: root

    property string address: ""
    property Offers source: Offers {}

    Column {
        id: column

        readonly property var offered: root.source !== null && root.address.length === 0 ? root.source.suggestions : []

        Repeater {
            model: column.offered

            delegate: AbstractButton {
                id: row

                required property var modelData
                required property int index
                readonly property bool recent: modelData.kind === "recent"

                objectName: "row-" + index
                text: modelData.title

                contentItem: Text {
                    objectName: "rowTitle"
                    text: row.modelData.title
                }
            }
        }
    }
}
