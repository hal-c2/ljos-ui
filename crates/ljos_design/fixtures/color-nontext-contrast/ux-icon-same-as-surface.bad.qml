// expect: color-nontext-contrast at 15
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 80
    Rectangle {
        anchors.fill: parent
        color: palette.window
        Button {
            anchors.centerIn: parent
            Accessible.name: "Record"
            background: Item {}
            contentItem: Rectangle { implicitWidth: 20; implicitHeight: 20; radius: 10; color: palette.window }
        }
    }
}
