// expect: a11y-unnamed-interactive at 9
import QtQuick

Rectangle {
    width: 120
    height: 40
    activeFocusOnTab: true
    Keys.onReturnPressed: console.log("open")
    MouseArea {
        anchors.fill: parent
        onClicked: console.log("open")
    }
}
