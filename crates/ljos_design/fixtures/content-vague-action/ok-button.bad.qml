// expect: content-vague-action at 8
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 80
    Button { text: qsTr("OK"); onClicked: console.log("saved") }
}
