import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 100
    Rectangle {
        width: 240; height: 40; color: palette.button
        activeFocusOnTab: true
        border.width: activeFocus ? 2 : 0
        border.color: palette.highlight
        Accessible.role: Accessible.Button
        Accessible.name: "Inbox"
        Label { anchors.centerIn: parent; text: "Inbox" }
        TapHandler { onTapped: console.log("open") }
        Keys.onReturnPressed: console.log("open")
    }
}
