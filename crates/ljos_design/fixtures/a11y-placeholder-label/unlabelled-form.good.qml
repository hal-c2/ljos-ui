import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

GridLayout {
    columns: 2
    Label { text: qsTr("Name") }
    TextField { id: name }
    Label { text: qsTr("Email") }
    TextField { placeholderText: qsTr("name@example.org") }
}
