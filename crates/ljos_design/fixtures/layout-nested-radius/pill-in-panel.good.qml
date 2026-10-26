import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    width: 200; height: 100
    radius: 6
    color: palette.window
    Rectangle {
        width: 40; height: 20
        radius: 12
        color: palette.base
    }
    Rectangle {
        anchors.fill: parent
        radius: parent.radius
        color: palette.base
    }
}
