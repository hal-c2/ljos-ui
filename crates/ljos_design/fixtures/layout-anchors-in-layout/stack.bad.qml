// expect: layout-anchors-in-layout at 9
import QtQuick
import QtQuick.Layouts

GridLayout {
    width: 300
    height: 200
    Rectangle {
        anchors.fill: parent
    }
}
