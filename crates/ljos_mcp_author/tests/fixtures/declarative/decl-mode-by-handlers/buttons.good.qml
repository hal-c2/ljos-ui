import QtQuick
import QtQuick.Controls

Column {
    id: root
    states: [
        State { name: "list"; PropertyChanges { list.opacity: 1; editor.opacity: 0 } },
        State { name: "edit"; PropertyChanges { list.opacity: 0; editor.opacity: 1 } }
    ]
    Button { text: qsTr("List"); onClicked: root.state = "list" }
    Button { text: qsTr("Edit"); onClicked: root.state = "edit" }
    Rectangle { id: list; width: 80; height: 40 }
    Rectangle { id: editor; width: 80; height: 40 }
}
