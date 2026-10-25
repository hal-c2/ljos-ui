// expect: a11y-tab-unreachable at 17
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
            Accessible.role: Accessible.Button
            Accessible.name: modelData
            Text { anchors.centerIn: parent; text: modelData; color: palette.text }
            TapHandler { onTapped: console.log(modelData) }
        }
    }
}
