import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 300
    ColumnLayout {
        anchors.fill: parent
        ListView {
            Layout.fillWidth: true; Layout.fillHeight: true; clip: true
            model: 20
            delegate: ItemDelegate { width: ListView.view.width; text: "Item " + index }
        }
        Row { spacing: 8; Button { text: "Cancel" } Button { text: "OK" } }
    }
}
