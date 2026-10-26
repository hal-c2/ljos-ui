import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 200
    function showUndo() { console.log("undo offered") }
    Menu {
        MenuItem { text: qsTr("Remove from list"); onTriggered: { console.log("removed"); showUndo() } }
        // ljos-allow(focus-destructive-undo): the list is rebuilt from the file on every start
        MenuItem { text: qsTr("Clear recent"); onTriggered: console.log("cleared") }
    }
}
