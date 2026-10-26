import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 200
    ListView {
        anchors.fill: parent
        model: ["Report.pdf", "Notes.txt"]
        delegate: RowLayout {
            width: ListView.view.width
            ItemDelegate {
                Layout.fillWidth: true
                text: modelData
                onClicked: console.log("open", modelData)
            }
            Button {
                text: qsTr("Delete")
                Accessible.name: qsTr("Delete %1").arg(modelData)
            }
        }
    }
}
