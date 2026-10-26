// audit: scheme=dark
import QtQuick
import QtQuick.Controls
Rectangle {
    readonly property bool dark: Qt.styleHints.colorScheme === Qt.ColorScheme.Dark
    width: 640; height: 480
    color: dark ? "#1e1e1e" : "white"
    Text {
        x: 8; y: 8
        text: "Last saved an hour ago"
        color: parent.dark ? "#b0b0b0" : "#595959"
    }
}
