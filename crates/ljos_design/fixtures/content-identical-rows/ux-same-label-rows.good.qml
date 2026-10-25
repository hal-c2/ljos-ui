import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 200
    ListView {
        anchors.fill: parent
        model: ["build", "test", "lint", "deploy", "clean"]
        delegate: ItemDelegate { width: ListView.view.width; text: "Run command: " + modelData }
    }
}
