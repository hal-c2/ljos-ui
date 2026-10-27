import QtQuick
import QtQuick.Controls

TextField {
    placeholderText: qsTr("Search")
    Accessible.name: qsTr("Search")
    onTextEdited: delay.restart()
    Timer { id: delay; interval: 300 }
}
