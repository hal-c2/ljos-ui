import QtQuick
import QtQuick.Controls.Basic
Rectangle {
    width: 640; height: 480
    color: "white"
    Row {
        spacing: 8
        Button { id: a; text: "One" }
        Button { id: b; text: "Two"; KeyNavigation.tab: c }
        Button { id: c; text: "Three"; KeyNavigation.tab: a }
    }
}
