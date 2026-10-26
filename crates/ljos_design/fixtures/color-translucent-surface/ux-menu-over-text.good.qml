import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 360; height: 240
    Text { anchors.fill: parent; wrapMode: Text.Wrap; color: palette.windowText; text: "Body text that runs behind the menu. ".repeat(12) }
    Menu {
        x: 20; y: 20
        Component.onCompleted: open()
        background: Rectangle { implicitWidth: 200; color: palette.window; border.color: palette.mid }
        MenuItem { text: "Cut" }
        MenuItem { text: "Copy" }
    }
}
