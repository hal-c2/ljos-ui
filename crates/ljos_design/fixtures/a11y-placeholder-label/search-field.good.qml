import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    width: 240
    TextField {
        placeholderText: qsTr("Search")
        Accessible.name: qsTr("Search")
    }
    Button { text: qsTr("Find") }
}
