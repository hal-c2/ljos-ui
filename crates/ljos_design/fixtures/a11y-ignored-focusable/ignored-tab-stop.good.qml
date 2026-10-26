import QtQuick
import QtQuick.Controls

Item {
    width: 400; height: 120
    Row {
        spacing: 8
        Rectangle {
            width: 120; height: 40; color: "#dddddd"
            Accessible.ignored: true
        }
        Button { text: qsTr("Next") }
    }
}
