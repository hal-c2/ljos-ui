import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 100
    ComboBox {
        id: c
        model: ["Small", "Large"]
        background: Rectangle {
            color: "transparent"
            border.width: c.visualFocus ? 2 : 0
            border.color: palette.highlight
        }
    }
}
