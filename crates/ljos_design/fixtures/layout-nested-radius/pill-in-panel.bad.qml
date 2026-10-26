// expect: layout-nested-radius at 12
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    width: 200; height: 100
    radius: 6
    color: palette.window
    Rectangle {
        anchors.fill: parent
        radius: 12
        color: palette.base
    }
}
