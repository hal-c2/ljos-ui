import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 100
    Button {
        id: b
        text: "Bold"; checkable: true
        background: Rectangle {
            color: b.checked ? palette.highlight : "transparent"
            border.color: palette.mid; border.width: b.checked ? 2 : 1
        }
        contentItem: Text {
            text: b.text; font.bold: b.checked
            color: b.checked ? palette.highlightedText : palette.windowText
        }
    }
}
