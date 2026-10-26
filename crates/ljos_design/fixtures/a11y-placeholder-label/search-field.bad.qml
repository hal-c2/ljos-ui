// expect: a11y-placeholder-label at 8
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    width: 240
    TextField { placeholderText: qsTr("Search") }
    Button { text: qsTr("Find") }
}
