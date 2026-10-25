// expect: layout-shift at 11
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: page
    width: 300; height: 300
    property bool loaded: false
    Timer { interval: 200; running: true; onTriggered: page.loaded = true }
    Label { text: "Projects"; font.bold: true; y: page.loaded ? 8 : page.height / 2 }
}
