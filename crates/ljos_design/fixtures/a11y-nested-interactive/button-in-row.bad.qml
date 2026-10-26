// expect: a11y-nested-interactive at 14
import QtQuick
import QtQuick.Controls

Item {
    width: 400; height: 200
    ListView {
        anchors.fill: parent
        model: ["Report.pdf", "Notes.txt"]
        delegate: ItemDelegate {
            width: ListView.view.width
            text: modelData
            onClicked: console.log("open", modelData)
            Button {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Delete")
                Accessible.name: qsTr("Delete %1").arg(modelData)
            }
        }
    }
}
