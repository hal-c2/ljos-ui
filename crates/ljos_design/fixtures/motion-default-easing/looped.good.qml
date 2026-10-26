import QtQuick

Rectangle {
    width: 40
    height: 40
    RotationAnimator on rotation {
        from: 0
        to: 360
        duration: 1000
        loops: Animation.Infinite
    }
}
