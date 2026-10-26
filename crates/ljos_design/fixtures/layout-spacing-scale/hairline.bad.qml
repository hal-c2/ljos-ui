// expect: layout-spacing-scale at 9
import QtQuick

Column {
    spacing: 1
    Rectangle {
        width: 100
        height: 20
        anchors.leftMargin: 10
    }
}
