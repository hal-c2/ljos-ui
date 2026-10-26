// expect: motion-default-easing at 7
import QtQuick

Rectangle {
    width: 40
    height: 40
    RotationAnimator on rotation {
        from: 0
        to: 90
        duration: 300
    }
}
