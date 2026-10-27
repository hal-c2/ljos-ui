import QtQuick

Item {
    id: root
    property string path: ""
    Component {
        id: page
        Item {
            property string path: root.path
            onPathChanged: root.path = path
        }
    }
}
