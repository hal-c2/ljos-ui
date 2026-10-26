// expect: a11y-tab-unreachable at 11
import QtQuick
import QtQuick.Controls
Rectangle {
    width: 640; height: 480
    color: "white"
    Row {
        spacing: 8
        Button { id: a; text: "One" }
        Button { id: b; text: "Two"; KeyNavigation.tab: a }
        Button { id: c; text: "Three" }
    }
}
