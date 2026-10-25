// expect: text-font-literal at 9, text-font-literal at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 100
    Column {
        Label { text: "Name"; font.family: "Courier New" }
        TextField { font.family: "Monospace" }
    }
}
