// expect: layout-anchors-in-layout at 10
import QtQuick
import QtQuick.Layouts

ColumnLayout {
    width: 300
    Repeater {
        model: 3
        Rectangle {
            anchors.left: parent.left
            implicitWidth: 100
            implicitHeight: 20
        }
    }
}
