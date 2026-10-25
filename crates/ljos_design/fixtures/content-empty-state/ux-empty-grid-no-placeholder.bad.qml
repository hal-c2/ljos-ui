// expect: content-empty-state at 11
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 200
    GridView {
        anchors.fill: parent
        cellWidth: 100; cellHeight: 100
        model: []
        delegate: Rectangle { width: 90; height: 90 }
    }
}
