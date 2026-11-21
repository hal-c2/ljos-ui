// expect: health-fault at 9
import QtQuick

Item {
    id: root
    property var row: null
    Component.onCompleted: {
        // The row is not there yet.
        root.row.title = "Inbox"
    }
}
