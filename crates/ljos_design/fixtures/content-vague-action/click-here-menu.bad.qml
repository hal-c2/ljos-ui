// expect: content-vague-action at 9, content-vague-action at 12
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 80
    Menu {
        MenuItem { text: qsTr("&Yes") }
        MenuItem { text: qsTr("Rename") }
    }
    Button { text: "Click here" }
}
