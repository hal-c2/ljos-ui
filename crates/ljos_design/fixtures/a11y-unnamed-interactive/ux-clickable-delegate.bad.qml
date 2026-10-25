// expect: a11y-unnamed-interactive at 15
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 200
    ListView {
        anchors.fill: parent
        model: ["Alpha", "Beta", "Gamma"]
        delegate: Rectangle {
            width: ListView.view.width; height: 36
            color: palette.base
            Text { anchors.centerIn: parent; text: modelData; color: palette.text }
            MouseArea { anchors.fill: parent; onClicked: console.log(modelData) }
        }
    }
}
