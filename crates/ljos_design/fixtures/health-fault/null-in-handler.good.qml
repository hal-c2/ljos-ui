import QtQuick

Item {
    id: root
    property var row: null
    Component.onCompleted: {
        if (root.row)
            root.row.title = "Inbox"
    }
}
