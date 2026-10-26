// expect: layout-under-minimum at 11
// audit: size=800x400
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 800; height: 400
    Row {
        Rectangle { id: panel; width: 540; height: 400; color: palette.window; border.color: palette.mid }
        Rectangle {
            implicitWidth: 420; width: parent.parent.width - panel.width; height: 400
            color: palette.base
            Label { text: "Viewer content"; anchors.centerIn: parent }
        }
    }
}
