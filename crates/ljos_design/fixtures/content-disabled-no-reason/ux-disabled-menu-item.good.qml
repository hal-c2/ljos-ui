import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 200
    Menu {
        Component.onCompleted: open()
        MenuItem { text: "Copy" }
        MenuItem {
            text: "Paste"; enabled: false
            ToolTip.visible: hovered; ToolTip.text: "Nothing to paste"
        }
    }
}
