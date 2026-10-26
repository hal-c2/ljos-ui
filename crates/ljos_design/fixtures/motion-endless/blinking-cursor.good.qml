import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 120; height: 40
    Label { id: caret; text: qsTr("|") }
    Timer {
        id: blink
        interval: 600
        repeat: true
        running: true
        onTriggered: caret.visible = !caret.visible
    }
    Timer { interval: 5000; running: true; onTriggered: blink.stop() }
}
