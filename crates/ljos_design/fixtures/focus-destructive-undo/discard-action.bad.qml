// expect: focus-destructive-undo at 9
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 80
    Action {
        text: qsTr("Discard changes")
        onTriggered: console.log("discarded")
    }
}
