// expect: focus-pointer-only at 11
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 100
    Rectangle {
        width: 200; height: 40; color: palette.button
        Label { anchors.centerIn: parent; text: "Expand" }
        MouseArea { anchors.fill: parent; onClicked: console.log("expand") }
    }
}
