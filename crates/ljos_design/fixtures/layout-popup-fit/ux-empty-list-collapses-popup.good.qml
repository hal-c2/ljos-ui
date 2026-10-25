import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 240
    Popup {
        Component.onCompleted: open()
        width: 200; height: Math.max(list.contentHeight, 56)
        ListView {
            id: list
            anchors.fill: parent; model: []
            Label { anchors.centerIn: parent; text: "No results" }
        }
    }
}
