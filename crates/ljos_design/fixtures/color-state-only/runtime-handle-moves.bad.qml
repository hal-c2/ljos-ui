// expect: color-state-only at 8
// audit: reduced-motion=false
import QtQuick
import QtQuick.Controls

Item {
    width: 300; height: 100
    Switch {
        id: control
        x: 20; y: 20
        Accessible.name: qsTr("Heating")
        // Only the track says the state: white or a light green, under 3:1.
        indicator: Rectangle {
            implicitWidth: 56; implicitHeight: 24
            radius: 12
            color: control.checked ? "#a8e6c4" : "#ffffff"
            border.color: "#b0b0b0"
            Rectangle {
                x: 2
                anchors.verticalCenter: parent.verticalCenter
                width: 20; height: 20; radius: 10
                color: "#ffffff"
                border.color: "#b0b0b0"
            }
        }
    }
}
