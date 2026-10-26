// expect: color-scheme-ignored at 14
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 100
    Switch {
        id: sw
        text: "Notifications"
        indicator: Rectangle {
            implicitWidth: 40; implicitHeight: 20; radius: 10
            x: sw.leftPadding; y: (sw.height - height) / 2
            color: sw.checked ? "#4caf50" : "#cccccc"
        }
    }
}
