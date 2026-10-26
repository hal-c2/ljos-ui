import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 80
    Action {
        text: qsTr("Discard changes")
        onTriggered: confirmDiscard()
    }
    function confirmDiscard() { console.log("ask first") }
}
