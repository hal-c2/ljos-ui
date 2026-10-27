// expect: decl-mode-by-handlers at 6
import QtQuick
import QtQuick.Controls

Column {
    Button { text: qsTr("List"); onClicked: { list.opacity = 1; editor.opacity = 0 } }
    Button { text: qsTr("Edit"); onClicked: { list.opacity = 0; editor.opacity = 1 } }
    Rectangle { id: list; width: 80; height: 40 }
    Rectangle { id: editor; width: 80; height: 40 }
}
