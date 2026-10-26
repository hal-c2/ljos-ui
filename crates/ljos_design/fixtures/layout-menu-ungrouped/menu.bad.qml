// expect: layout-menu-ungrouped at 6
import QtQuick
import QtQuick.Controls

Item {
    Menu {
        MenuItem { text: qsTr("New") }
        MenuItem { text: qsTr("Open") }
        MenuItem { text: qsTr("Open Recent") }
        MenuItem { text: qsTr("Save") }
        MenuItem { text: qsTr("Save As") }
        MenuItem { text: qsTr("Export") }
        MenuItem { text: qsTr("Print") }
        MenuItem { text: qsTr("Share") }
        MenuItem { text: qsTr("Properties") }
        MenuItem { text: qsTr("Close") }
        MenuItem { text: qsTr("Quit") }
    }
}
