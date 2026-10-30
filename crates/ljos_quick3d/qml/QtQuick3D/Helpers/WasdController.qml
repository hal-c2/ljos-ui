import QtQuick
import QtQuick3D

// Moves a node with the keys and turns it with the pointer, as in a
// first-person view. The frame animation runs only while a key is held,
// so a scene nobody is moving through costs nothing.
Item {
    id: root

    property Node controlledObject: null

    property real speed: 1
    property real shiftSpeed: 3
    property real forwardSpeed: 5
    property real backSpeed: 5
    property real leftSpeed: 5
    property real rightSpeed: 5
    property real upSpeed: 5
    property real downSpeed: 5

    property real xSpeed: 0.1
    property real ySpeed: 0.1
    property bool xInvert: false
    property bool yInvert: true

    property bool keysEnabled: true
    property bool mouseEnabled: true
    property int acceptedButtons: Qt.LeftButton

    // True while a key is held that moves the object.
    readonly property bool inputsNeedProcessing: held.forward || held.back || held.left
        || held.right || held.up || held.down

    focus: keysEnabled

    QtObject {
        id: held
        property bool forward: false
        property bool back: false
        property bool left: false
        property bool right: false
        property bool up: false
        property bool down: false
        property bool shift: false
    }

    function hold(key, modifiers, down) {
        if (!keysEnabled)
            return false
        held.shift = (modifiers & Qt.ShiftModifier) !== 0
        switch (key) {
        case Qt.Key_W: case Qt.Key_Up: held.forward = down; break
        case Qt.Key_S: case Qt.Key_Down: held.back = down; break
        case Qt.Key_A: case Qt.Key_Left: held.left = down; break
        case Qt.Key_D: case Qt.Key_Right: held.right = down; break
        case Qt.Key_R: case Qt.Key_PageUp: held.up = down; break
        case Qt.Key_F: case Qt.Key_PageDown: held.down = down; break
        case Qt.Key_Shift: break
        default: return false
        }
        return true
    }

    Keys.onPressed: event => event.accepted = hold(event.key, event.modifiers, true)
    Keys.onReleased: event => event.accepted = hold(event.key, event.modifiers, false)

    // Per-frame speeds in the documentation are for a 60 Hz display;
    // scaling by the frame time keeps the pace on any other.
    function step(seconds) {
        if (!controlledObject)
            return
        const factor = speed * (held.shift ? shiftSpeed : 1) * seconds * 60
        let move = Qt.vector3d(0, 0, 0)
        if (held.forward)
            move = move.plus(controlledObject.forward.times(forwardSpeed))
        if (held.back)
            move = move.minus(controlledObject.forward.times(backSpeed))
        if (held.right)
            move = move.plus(controlledObject.right.times(rightSpeed))
        if (held.left)
            move = move.minus(controlledObject.right.times(leftSpeed))
        if (held.up)
            move = move.plus(controlledObject.up.times(upSpeed))
        if (held.down)
            move = move.minus(controlledObject.up.times(downSpeed))
        // The scene's directions, taken to the object's parent's.
        const parent = controlledObject.parent
        if (parent && parent.mapDirectionFromScene)
            move = parent.mapDirectionFromScene(move)
        controlledObject.position = controlledObject.position.plus(move.times(factor))
    }

    FrameAnimation {
        running: root.inputsNeedProcessing
        onTriggered: root.step(frameTime)
    }

    DragHandler {
        enabled: root.mouseEnabled
        target: null
        acceptedButtons: root.acceptedButtons
        onTranslationChanged: delta => {
            if (!root.controlledObject)
                return
            const rotation = root.controlledObject.eulerRotation
            const pitch = rotation.x + (root.yInvert ? delta.y : -delta.y) * root.ySpeed
            root.controlledObject.eulerRotation = Qt.vector3d(
                Math.max(-90, Math.min(90, pitch)),
                rotation.y + (root.xInvert ? delta.x : -delta.x) * root.xSpeed,
                rotation.z)
        }
    }
}
