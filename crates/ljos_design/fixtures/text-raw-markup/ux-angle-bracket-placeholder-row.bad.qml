// expect: text-raw-markup at 11
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 160
    ListView {
        anchors.fill: parent
        model: ["<default>", "Fast", "Accurate"]
        delegate: ItemDelegate { width: ListView.view.width; text: modelData }
    }
}
