import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 360; height: 100
    Text {
        anchors.centerIn: parent; color: palette.windowText
        textFormat: Text.MarkdownText
        text: "Run `make build` before **continuing**"
    }
}
