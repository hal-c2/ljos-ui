// expect: layout-shift at 12
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: page
    width: 300; height: 300
    property bool busy: false
    Timer { interval: 200; running: true; onTriggered: page.busy = true }
    Column {
        Label { text: "Saving…"; visible: page.busy }
        Button { text: "Row one" }
        Button { text: "Row two" }
    }
}
