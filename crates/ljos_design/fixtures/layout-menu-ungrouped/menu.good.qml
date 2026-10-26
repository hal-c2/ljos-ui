import QtQuick
import QtQuick.Controls

Menu {
    MenuItem { text: qsTr("New") }
    MenuItem { text: qsTr("Open") }
    MenuItem { text: qsTr("Open Recent") }
    MenuSeparator {}
    MenuItem { text: qsTr("Save") }
    MenuItem { text: qsTr("Save As") }
    MenuItem { text: qsTr("Export") }
    MenuSeparator {}
    MenuItem { text: qsTr("Print") }
    MenuItem { text: qsTr("Share") }
    MenuItem { text: qsTr("Properties") }
    MenuSeparator {}
    MenuItem { text: qsTr("Close") }
    MenuItem { text: qsTr("Quit") }
}
