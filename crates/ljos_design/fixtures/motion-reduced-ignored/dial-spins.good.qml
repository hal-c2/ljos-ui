import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

BusyIndicator {
    running: true
    contentItem: Item {
        implicitWidth: 32; implicitHeight: 32
        Item {
            anchors.fill: parent
            RotationAnimator on rotation { from: 0; to: 360; duration: 1000; loops: Animation.Infinite; running: true }
        }
    }
}
