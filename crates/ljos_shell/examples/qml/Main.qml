import QtQuick
import QtQuick.Controls.Basic

Item {
    id: root

    property int clicks: 0

    Column {
        x: 24
        y: 24
        spacing: 12

        Text {
            text: "Ljós on egui, from QML"
            font.pixelSize: 24
            color: "#212121"
        }

        Row {
            spacing: 8
            Rectangle { width: 48; height: 48; color: "#2e7d32" }
            Rectangle { width: 48; height: 48; radius: 8; color: "#c62828" }
            Rectangle { width: 48; height: 48; radius: 16; color: "#1565c0" }
            Rectangle { width: 48; height: 48; radius: 24; color: "#ef6c00" }
        }

        // A 16 by 16 picture carried in its URL, decoded off the UI thread
        // and tiled.
        Image {
            width: 216
            height: 48
            fillMode: Image.Tile
            smooth: false
            source: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAIAAACQkWg2AAAAIUlEQVR42mN4n8OAFf3/8RArYhjVQBMNuCRwGTSqgSYaAMXJGZ+uT/rTAAAAAElFTkSuQmCC"
        }
    }

    Rectangle {
        anchors.centerIn: parent
        width: 220
        height: 90
        radius: 10
        color: "#fff8e1"
        border.width: 2
        border.color: "#ef6c00"

        Text {
            anchors.centerIn: parent
            text: "anchors.centerIn: parent"
        }
    }

    // Controls as the Basic style draws them.
    Column {
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        anchors.margins: 24
        spacing: 8

        Row {
            spacing: 8
            Button { text: "Button" }
            Button { text: "Checked"; checkable: true; checked: true }
            ToolButton { text: "Tool" }
        }

        Row {
            spacing: 8
            CheckBox { text: "Check box"; checked: true }
            RadioButton { text: "One"; checked: true }
            RadioButton { text: "Two" }
            Switch { text: "Switch"; checked: true }
        }
    }

    // A button whose parts the application draws itself.
    Button {
        id: button
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 24
        text: root.clicks > 0 ? "Clicked " + root.clicks + "×" : "Click me"
        opacity: hovered ? 0.85 : 1.0
        onClicked: root.clicks++

        background: Rectangle {
            implicitWidth: 160
            implicitHeight: 56
            radius: 6
            color: root.clicks % 2 ? "#c62828" : "#1565c0"
        }

        contentItem: Text {
            text: button.text
            color: "white"
            font.pixelSize: 16
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
    }
}
