// expect: focus-destructive-undo at 9
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 200
    Menu {
        MenuItem { text: qsTr("Remove from list"); onTriggered: console.log("removed") }
        MenuItem { text: qsTr("Copy"); onTriggered: console.log("copied") }
    }
}
