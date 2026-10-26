// expect: focus-destructive-undo at 8
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 80
    Button { text: qsTr("Delete project"); onClicked: console.log("deleted") }
}
