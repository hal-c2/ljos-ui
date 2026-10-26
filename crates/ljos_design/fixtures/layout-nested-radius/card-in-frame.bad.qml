// expect: layout-nested-radius at 13
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    width: 200; height: 100
    radius: 4
    color: palette.window
    Rectangle {
        anchors.fill: parent
        anchors.margins: 2
        radius: 8
        color: palette.base
    }
}
