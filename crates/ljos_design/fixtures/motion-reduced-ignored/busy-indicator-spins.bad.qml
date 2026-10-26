// expect: motion-reduced-ignored at 11
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 48; height: 48
    Item {
        id: spinner
        anchors.fill: parent
        RotationAnimator on rotation { from: 0; to: 360; duration: 1000; loops: Animation.Infinite }
    }
}
