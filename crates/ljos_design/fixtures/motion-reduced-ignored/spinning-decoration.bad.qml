// expect: motion-reduced-ignored at 12
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: decoration
    width: 48; height: 48
    Label {
        anchors.centerIn: parent
        text: qsTr("*")
        RotationAnimation on rotation {
            from: 0; to: 360; duration: 1200
            loops: Animation.Infinite
        }
    }
}
