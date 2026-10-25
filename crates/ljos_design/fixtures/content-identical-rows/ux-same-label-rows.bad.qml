// expect: content-identical-rows at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 200
    ListView {
        anchors.fill: parent
        model: 5
        delegate: ItemDelegate { width: ListView.view.width; text: "Run command" }
    }
}
