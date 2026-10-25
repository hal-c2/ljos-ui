import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 200
    Menu {
        width: 200
        Component.onCompleted: open()
        MenuItem {
            text: "Unavailable"; enabled: false
            ToolTip.visible: hovered; ToolTip.text: "The account has reached its limit"
        }
    }
}
