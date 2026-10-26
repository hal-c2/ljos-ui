// expect: motion-endless at 9
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 120; height: 40
    Label { id: caret; text: qsTr("|") }
    Timer {
        interval: 600
        repeat: true
        running: true
        onTriggered: caret.visible = !caret.visible
    }
}
