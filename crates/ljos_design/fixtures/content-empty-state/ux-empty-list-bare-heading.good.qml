import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 200
    ListView {
        id: list
        anchors.fill: parent
        model: 0
        header: Label { text: "Recent files"; font.bold: true }
        delegate: ItemDelegate { text: "Row" }
        Label { anchors.centerIn: parent; visible: list.count === 0; text: "No recent files yet" }
    }
}
