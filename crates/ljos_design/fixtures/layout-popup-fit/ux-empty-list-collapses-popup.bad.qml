// expect: layout-popup-fit at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 240
    Popup {
        Component.onCompleted: open()
        width: 200; height: list.contentHeight
        ListView {
            id: list
            anchors.fill: parent; model: []
            Label { anchors.centerIn: parent; text: "No results" }
        }
    }
}
