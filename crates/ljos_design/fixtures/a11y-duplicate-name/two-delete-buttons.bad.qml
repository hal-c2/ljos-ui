// expect: a11y-duplicate-name at 10
import QtQuick
import QtQuick.Controls.Basic
Rectangle {
    width: 640; height: 480
    color: "white"
    Row {
        spacing: 8
        Button { text: "Delete" }
        Button { text: "Delete" }
    }
}
