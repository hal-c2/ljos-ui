import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 160
    Flow {
        width: parent.width; spacing: 6
        Repeater {
            model: ["Cut", "Copy", "Paste", "Undo"]
            Button { text: modelData }
        }
    }
}
