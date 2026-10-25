import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 200
    ListView {
        anchors.fill: parent; clip: true
        model: 50
        currentIndex: 40
        delegate: ItemDelegate { width: ListView.view.width; text: "Row " + index; highlighted: ListView.isCurrentItem }
        Component.onCompleted: positionViewAtIndex(currentIndex, ListView.Contain)
    }
}
