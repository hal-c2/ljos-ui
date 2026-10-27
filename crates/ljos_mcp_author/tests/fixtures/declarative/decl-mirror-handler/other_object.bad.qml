// expect: decl-mirror-handler at 10
import QtQuick
import QtQuick.Controls

Column {
    TextField {
        id: field
        placeholderText: qsTr("Filter")
        Accessible.name: qsTr("Filter")
        onTextChanged: list.filter = text.trim()
    }
    Item { id: list; property string filter: "" }
}
