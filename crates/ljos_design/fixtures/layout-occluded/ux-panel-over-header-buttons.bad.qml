// expect: layout-occluded at 13
// audit: size=600x400
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 600; height: 400
    Row {
        spacing: 4
        Repeater {
            model: ["Cut", "Copy", "Paste", "Undo", "Redo", "Find"]
            Button { text: modelData }
        }
    }
    Rectangle {
        anchors.right: parent.right; height: parent.height; width: 360; z: 1
        color: palette.window; border.color: palette.mid
    }
}
