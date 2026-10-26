import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    width: 240
    Label { text: qsTr("Note") }
    TextArea { placeholderText: qsTr("Write a note") }
}
