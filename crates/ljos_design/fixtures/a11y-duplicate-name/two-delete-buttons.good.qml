import QtQuick
import QtQuick.Controls
Rectangle {
    width: 640; height: 480
    color: "white"
    Row {
        spacing: 8
        Button { text: "Delete draft" }
        Button { text: "Delete all" }
    }
}
