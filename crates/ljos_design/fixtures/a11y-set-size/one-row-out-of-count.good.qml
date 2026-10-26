import QtQuick
import QtQuick.Controls
Rectangle {
    width: 640; height: 480
    color: "white"
    Column {
        Accessible.role: Accessible.List
        Accessible.name: "Files"
        Repeater {
            model: 3
            Rectangle {
                width: 100; height: 30
                Accessible.role: Accessible.ListItem
                Accessible.name: "File " + (index + 1)
                Accessible.focusable: false
            }
        }
    }
}
