import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 500
    ListModel { id: results; ListElement { name: "One" } }
    Popup {
        width: 320; implicitHeight: Math.min(list.contentHeight + topPadding + bottomPadding, 360)
        ListView { id: list; anchors.fill: parent; model: results; delegate: ItemDelegate { text: name; width: ListView.view.width } }
    }
}
