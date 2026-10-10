import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    MouseArea {
        anchors.fill: parent
        onClicked: console.log("hit")
    }

    Rectangle {
        Accessible.name: qsTr("Named")
        MouseArea {
            anchors.fill: parent
            onClicked: console.log("covered by the parent")
        }
    }

    MouseArea {
        hoverEnabled: true
    }

    Item {
        TapHandler {
            onTapped: console.log("tap")
        }
    }

    ToolButton {
        icon.source: "close.svg"
    }

    ToolButton {
        icon.name: "edit-copy"
        Accessible.name: qsTr("Copy")
    }

    Button {
        icon.source: "ok.svg"
        text: qsTr("OK")
    }

    Image {
        source: "photo.png"
    }

    Image {
        source: "divider.png"
        Accessible.ignored: true
    }
}
