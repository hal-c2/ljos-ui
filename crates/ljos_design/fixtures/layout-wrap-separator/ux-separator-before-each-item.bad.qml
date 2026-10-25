// expect: layout-wrap-separator at 14
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 160
    Flow {
        width: parent.width; spacing: 6
        Repeater {
            model: ["Cut", "Copy", "Paste", "Undo"]
            Row {
                spacing: 6
                Rectangle { width: 1; height: 24; color: palette.mid }
                Button { text: modelData }
            }
        }
    }
}
