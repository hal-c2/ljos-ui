import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: page
    width: 300; height: 300
    property bool busy: false
    Timer { interval: 200; running: true; onTriggered: page.busy = true }
    Column {
        Button { text: "Row one" }
        Button { text: "Row two" }
        Label { text: "Saving…"; opacity: page.busy ? 1 : 0 }
    }
}
