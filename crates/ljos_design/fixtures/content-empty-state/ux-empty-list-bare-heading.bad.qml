// expect: content-empty-state at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 200
    ListView {
        anchors.fill: parent
        model: 0
        header: Label { text: "Recent files"; font.bold: true }
        delegate: ItemDelegate { text: "Row" }
    }
}
