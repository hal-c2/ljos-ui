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
    // A running animation keeps the duration it started with, and the
    // system's answer about reduced motion can come after the turntable
    // has started: a new speed starts the turn again from where the watch
    // is, so it neither jumps nor keeps the old speed. The handler gives
    // the turntable its new duration itself: the binding on `duration`
    // follows only once the handler has run, as on Qt.
    onTurnSecondsChanged: {
        const at: number = showcase.spin % 360
        turntable.from = at
        turntable.to = at + 360
        turntable.duration = showcase.turnSeconds * 1000
        turntable.restart()
        // A restart forgets that the turntable was paused, and `paused`'s
        // binding does not run again while the drag goes on.
        if (scene.dragging)
            turntable.pause()
    }
    NumberAnimation on spin {
        id: turntable
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
    // the screen's texture; it is not shown flat as well. Nobody can touch
    // it here, so it never sleeps to its dim face.
    Main {
        id: screen
        width: 454
        height: 454
        visible: false
        sleeps: false
    }
}
