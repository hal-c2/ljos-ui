import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: page
    width: 300; height: 300
    property bool loaded: false
    Timer { interval: 200; running: true; onTriggered: page.loaded = true }
    Label { text: "Projects"; font.bold: true; y: 8 }
    BusyIndicator { anchors.centerIn: parent; running: !page.loaded; visible: running }
}
