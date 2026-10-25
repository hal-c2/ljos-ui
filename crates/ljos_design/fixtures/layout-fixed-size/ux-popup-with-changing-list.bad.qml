// expect: layout-fixed-size at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 500
    ListModel { id: results; ListElement { name: "One" } }
    Popup {
        width: 320; height: 360
        ListView { anchors.fill: parent; model: results; delegate: ItemDelegate { text: name; width: ListView.view.width } }
    }
}
