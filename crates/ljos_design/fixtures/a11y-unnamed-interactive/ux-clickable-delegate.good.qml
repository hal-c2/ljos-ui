import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 200
    ListView {
        anchors.fill: parent
        model: ["Alpha", "Beta", "Gamma"]
        delegate: ItemDelegate {
            width: ListView.view.width
            text: modelData
            onClicked: console.log(modelData)
        }
    }
}
