import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 80
    Menu {
        MenuItem { text: qsTr("Delete 3 files") }
        MenuItem { text: qsTr("Rename") }
    }
    Button { text: qsTr("Open the manual") }
}
