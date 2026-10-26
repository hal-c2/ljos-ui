import QtQuick
import QtQuick.Layouts

ColumnLayout {
    width: 300
    Repeater {
        model: 3
        Rectangle {
            Layout.alignment: Qt.AlignLeft
            implicitWidth: 100
            implicitHeight: 20
        }
    }
}
