// expect: focus-indicator-missing at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 100
    Rectangle {
        width: 240; height: 40; color: palette.button
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: "Inbox"
        Label { anchors.centerIn: parent; text: "Inbox" }
        TapHandler { onTapped: console.log("open") }
        Keys.onReturnPressed: console.log("open")
    }
}
