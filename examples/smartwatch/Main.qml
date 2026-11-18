import Ljos.Ui
import Ljos.Core
import Ljos.Design
import "logic/dial.ts" as Dial

// Kestrel, a made-up watch: its software, for a round screen of 454 by
// 454 pixels and a rounded-square one of 410 by 502. Each screen lays
// itself out for the shape it is given.
//
// The crown is the mouse wheel and the side button Escape (or Home). Left
// alone for fifteen seconds the watch sleeps to its always-on face, and
// then only the clock wakes, once a minute. Nothing else runs at rest: a
// second hand ticks only while it is seen, the heart is sampled only on
// the screens that show it, and a countdown's one wake is its end.
Rectangle {
    id: display
    objectName: "display"

    implicitWidth: 454
    implicitHeight: 454
    fill: theme.canvas
    focus: true

    readonly property bool round: Math.abs(display.width - display.height) < 20
    // How far in from the edge text keeps, so a round screen's curve never
    // cuts it.
    readonly property real inset: display.round ? 46 : 26
    readonly property real corner: display.round ? display.width / 2 : 96
    // Asleep with always on off: the screen is off.
    readonly property bool dark: watch.ambient && !watch.alwaysOn
    // Whether, left alone, the watch sleeps. A watch shown off where nobody
    // can touch it, as on the 3D model, stays awake.
    property bool sleeps: true

    Theme {
        id: theme
        objectName: "theme"
        bloom: watch.bloom
    }
    WallClock {
        id: clock
        objectName: "clock"
    }
    Watch {
        id: watch
        objectName: "watch"
        clock: clock
    }

    // A second hand, a running count: only while awake and shown.
    Timer {
        objectName: "tick"
        interval: watch.tickEvery
        repeat: true
        running: watch.ticking
        onTriggered: {
            clock.read()
            watch.refresh()
        }
    }
    // The heart is sampled only on a screen that shows it.
    Timer {
        objectName: "beat"
        interval: 1000
        repeat: true
        running: watch.sampling
        onTriggered: watch.sample()
    }
    // Untouched this long, the watch sleeps. Every touch starts it over.
    Timer {
        id: idle
        objectName: "idle"
        interval: 15000
        running: display.sleeps
        onTriggered: watch.sleep()
    }
    Connections {
        target: watch
        function onTouched() {
            if (display.sleeps)
                idle.restart()
        }
    }
    // A countdown wakes the watch once, at its end.
    Timer {
        objectName: "ring"
        interval: Math.max(1, watch.timerDue - Date.now())
        running: watch.counting
        onTriggered: watch.ring()
    }

    Keys.onPressed: (event: KeyEvent) => {
        if (event.key === Key.Escape || event.key === Key.Home) {
            watch.pressSide()
            event.accepted = true
        } else if (event.key === Key.Return || event.key === Key.Enter) {
            watch.select()
            event.accepted = true
        }
    }

    // The crown: a notch of the wheel is a notch of the crown.
    Wheel {
        onWheel: (event: WheelEvent) => {
            const notches = Math.round(-event.angleDelta.y / 120)
            watch.turnCrown(notches !== 0 ? notches : (event.angleDelta.y < 0 ? 1 : -1))
        }
    }

    Pages {
        id: pages
        objectName: "pages"
        anchors.fill: parent
        theme: theme
        watch: watch
        clock: clock
        round: display.round
        inset: display.inset
        visible: !watch.ambient
    }

    AmbientFace {
        anchors.fill: parent
        theme: theme
        watch: watch
        clock: clock
        round: display.round
        visible: watch.ambient && !display.dark
    }

    // Asleep, a touch anywhere wakes the watch: the press itself, so a hold
    // or the start of a swipe wakes it as a tap does.
    Item {
        anchors.fill: parent
        visible: watch.ambient
        Accessible.role: Accessible.Button
        Accessible.name: t`Wake`
        Accessible.onPressAction: watch.touch()
        Tap {
            onPressedChanged: {
                if (pressed)
                    watch.touch()
            }
        }
    }

    // The glass ends where the screen does: everything outside the circle,
    // or the rounded square, is the bezel.
    Shape {
        anchors.fill: parent
        ShapePath {
            fillColor: theme.canvas
            strokeColor: "transparent"
            fillRule: ShapePath.OddEvenFill
            PathSvg {
                path: Dial.bezel(display.width, display.height, display.round, display.corner)
            }
        }
    }
}
