import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 200
    GridView {
        id: grid
        anchors.fill: parent
        cellWidth: 100; cellHeight: 100
        model: []
        delegate: Rectangle { width: 90; height: 90 }
        Label { anchors.centerIn: parent; visible: grid.count === 0; text: "Nothing to show" }
    }
}
