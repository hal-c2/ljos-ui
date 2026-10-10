import QtQuick

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

    Rectangle {
        id: button
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 24
        width: 160
        height: 56
        radius: 6
        color: root.clicks % 2 ? "#c62828" : "#1565c0"
        opacity: area.containsMouse ? 0.85 : 1.0

        Text {
            anchors.centerIn: parent
            color: "white"
            font.pixelSize: 16
            text: root.clicks > 0 ? "Clicked " + root.clicks + "×" : "Click me"
        }

        MouseArea {
            id: area
            anchors.fill: parent
            hoverEnabled: true
            onClicked: root.clicks++
        }
    }
}
