import QtQuick
import QtQuick3D

// Turns the origin node about its own axes when the pointer drags, moves
// the camera along the origin's -Z when the wheel turns or a pinch
// closes, and moves the origin across the view when a drag is made with
// Ctrl held. Nothing runs between events.
Item {
    id: root

    property Node origin: null
    property Node camera: null

    property real xSpeed: 0.1
    property real ySpeed: 0.1
    property bool xInvert: false
    property bool yInvert: true
    property bool mouseEnabled: true
    property bool panEnabled: true
    property bool automaticClipping: true
    property int acceptedButtons: Qt.LeftButton

    // Where the camera may stand from the origin.
    readonly property real minimumDistance: 1

    function orbit(dx, dy) {
        if (!origin)
            return
        const rotation = origin.eulerRotation
        const pitch = rotation.x + (yInvert ? dy : -dy) * ySpeed
        origin.eulerRotation = Qt.vector3d(Math.max(-90, Math.min(90, pitch)),
                                           rotation.y + (xInvert ? dx : -dx) * xSpeed,
                                           rotation.z)
    }

    function pan(dx, dy) {
        if (!origin || !camera || !panEnabled)
            return
        // A pixel of drag moves the origin by what a pixel covers at the
        // origin's distance.
        const reach = Math.abs(camera.z) * 0.002
        const right = origin.right
        const up = origin.up
        origin.position = origin.position.plus(
            right.times(-dx * reach * (xInvert ? -1 : 1)).plus(
            up.times(dy * reach * (yInvert ? 1 : -1))))
    }

    function zoom(factor) {
        if (!camera)
            return
        camera.z = Math.max(minimumDistance, camera.z * factor)
        if (automaticClipping) {
            // Near and far follow the distance so that the scene's depth
            // is spent where the scene is.
            camera.clipNear = Math.max(0.01, camera.z / 100)
            camera.clipFar = Math.max(camera.clipNear * 10, camera.z * 100)
        }
    }

    DragHandler {
        id: rotate
        enabled: root.mouseEnabled
        target: null
        acceptedButtons: root.acceptedButtons
        acceptedModifiers: Qt.NoModifier
        onTranslationChanged: delta => root.orbit(delta.x, delta.y)
    }

    DragHandler {
        id: move
        enabled: root.mouseEnabled && root.panEnabled
        target: null
        acceptedButtons: root.acceptedButtons
        acceptedModifiers: Qt.ControlModifier
        onTranslationChanged: delta => root.pan(delta.x, delta.y)
    }

    DragHandler {
        id: twoFingers
        enabled: root.mouseEnabled && root.panEnabled
        target: null
        minimumPointCount: 2
        maximumPointCount: 2
        onTranslationChanged: delta => root.pan(delta.x, delta.y)
    }

    WheelHandler {
        enabled: root.mouseEnabled
        onWheel: event => root.zoom(Math.pow(0.9, event.angleDelta.y / 120))
    }

    PinchHandler {
        id: pinch
        enabled: root.mouseEnabled
        target: null
        property real last: 1
        onActiveChanged: last = 1
        onActiveScaleChanged: {
            if (active && activeScale > 0) {
                root.zoom(last / activeScale)
                last = activeScale
            }
        }
    }
}
