// expect: layout-resize-handle at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 200
    SplitView {
        anchors.fill: parent
        handle: Item { implicitWidth: 6 }
        Rectangle { SplitView.preferredWidth: 150; color: palette.window }
        Rectangle { SplitView.fillWidth: true; color: palette.window }
    }
}
