// expect: a11y-placeholder-label at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

GridLayout {
    columns: 2
    Label { text: qsTr("Name") }
    TextField { id: name }
    TextField { placeholderText: qsTr("name@example.org") }
    Button { text: qsTr("Add") }
}
