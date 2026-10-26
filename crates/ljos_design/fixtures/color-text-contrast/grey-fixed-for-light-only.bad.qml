// expect: color-text-contrast at 9
// audit: scheme=dark
import QtQuick
import QtQuick.Controls
Rectangle {
    width: 640; height: 480
    // The page follows the scheme, the text does not.
    color: Qt.styleHints.colorScheme === Qt.ColorScheme.Dark ? "#1e1e1e" : "white"
    Text {
        x: 8; y: 8
        text: "Last saved an hour ago"
        color: "#595959"
    }
}
