import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 200
    SplitView {
        anchors.fill: parent
        handle: Rectangle {
            implicitWidth: 6
            color: SplitHandle.pressed ? palette.highlight : (SplitHandle.hovered ? palette.mid : palette.midlight)
        }
        Rectangle { SplitView.preferredWidth: 150; color: palette.window }
        Rectangle { SplitView.fillWidth: true; color: palette.window }
    }
}
