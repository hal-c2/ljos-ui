import Ljos.Ui
import Ljos.Core
import Ljos.Design

// The watch in 3D, wearing its own software: the running Main is the
// screen's picture, so whatever it shows (the clock ticking, a page
// sliding in) shows on the model too. The watch turns slowly on a
// turntable. Drag to turn it yourself; the turntable waits while you do.
// With reduced motion it still turns, but much more slowly.
Rectangle {
    id: showcase
    objectName: "showcase"

    implicitWidth: 900
    implicitHeight: 900
    // The watch's own always-dark ground, a shade off its black screen so
    // the case reads against it.
    fill: theme.sunken

    Theme {
        id: theme
        objectName: "theme"
    }

    // Seconds for one turn of the turntable.
    readonly property real turnSeconds: Motion.reduced ? 120 : 24
    property real spin: 0
    NumberAnimation on spin {
        from: 0
        to: 360
        duration: showcase.turnSeconds * 1000
        loops: Animation.Infinite
        paused: scene.dragging
    }

    WatchScene {
        id: scene
        anchors.fill: parent
        yaw: -24 + showcase.spin
        pitch: 12
        bloom: theme.bloom
        dark: theme.dark
        face: screen
    }

    // The watch's software at its round screen's size. It draws only into
    // the screen's texture; it is not shown flat as well.
    Main {
        id: screen
        width: 454
        height: 454
        visible: false
    }
}
