import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 80
    Rectangle {
        anchors.fill: parent
        color: palette.window
        Rectangle { anchors.centerIn: parent; width: 20; height: 20; radius: 10; color: palette.windowText }
    }
}
