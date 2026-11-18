import Ljos.Ui
import Ljos.Core
import Ljos.Design

// The screens, one at a time, and the swipes between them. A new screen
// comes in from where the finger sent it: from below for notifications,
// from above for quick settings, from the right going in and from the
// left coming back. With reduced motion it still comes in, from much
// nearer and without growing.
WatchScreen {
    id: pages

    // 0 when a screen has just been asked for, 1 when it is in place.
    property real shift: 1
    // Where the new screen comes from: -1 left or above, 1 right or below.
    property int fromX: 0
    property int fromY: 0
    readonly property real travel: Motion.reduced ? 18 : Math.min(pages.width, pages.height) * 0.32

    property string shown: pages.watch.page
    onShownChanged: {
        const to = pages.shown
        // The screen the watch starts on is simply there.
        if (to === pages.cameFrom) {
            return
        }
        pages.fromX = to === "face" || to === "notifications" && pages.cameFrom === "notification" ? -1
            : to === "notifications" || to === "settings" ? 0 : 1
        pages.fromY = to === "notifications" && pages.cameFrom === "face" ? 1
            : to === "settings" ? -1 : 0
        pages.cameFrom = to
        enter.restart()
    }
    property string cameFrom: "face"

    NumberAnimation {
        id: enter
        target: pages
        property: "shift"
        from: 0
        to: 1
        duration: Motion.enterScreen
        easing.type: Easing.OutCubic
    }

    // The finger: a drag that went far enough is a swipe.
    property real dx: 0
    property real dy: 0
    Drag {
        id: swipe
        onTranslationChanged: {
            pages.dx = swipe.translation.x
            pages.dy = swipe.translation.y
        }
        onActiveChanged: {
            if (!swipe.active) {
                pages.watch.swiped(pages.dx, pages.dy)
                pages.dx = 0
                pages.dy = 0
            } else {
                pages.watch.touch()
            }
        }
    }

    Item {
        id: stage
        width: pages.width
        height: pages.height
        x: (1 - pages.shift) * pages.travel * pages.fromX
        y: (1 - pages.shift) * pages.travel * pages.fromY
        opacity: 0.2 + 0.8 * pages.shift
        scale: Motion.reduced ? 1 : 0.94 + 0.06 * pages.shift

        FacePulse {
            anchors.fill: parent
            visible: pages.watch.page === "face" && pages.watch.face === 0
            theme: pages.theme
            watch: pages.watch
            clock: pages.clock
            round: pages.round
            inset: pages.inset
        }
        FaceGrid {
            anchors.fill: parent
            visible: pages.watch.page === "face" && pages.watch.face === 1
            theme: pages.theme
            watch: pages.watch
            clock: pages.clock
            round: pages.round
            inset: pages.inset
        }
        // Holding the face changes it.
        Tap {
            enabled: pages.watch.page === "face"
            onLongPressed: pages.watch.nextFace()
            onTapped: pages.watch.touch()
        }
        NoteList {
            anchors.fill: parent
            visible: pages.watch.page === "notifications"
            theme: pages.theme
            watch: pages.watch
            clock: pages.clock
            round: pages.round
            inset: pages.inset
        }
        NoteDetail {
            anchors.fill: parent
            visible: pages.watch.page === "notification"
            theme: pages.theme
            watch: pages.watch
            clock: pages.clock
            round: pages.round
            inset: pages.inset
        }
        HeartPage {
            anchors.fill: parent
            visible: pages.watch.page === "health"
            theme: pages.theme
            watch: pages.watch
            clock: pages.clock
            round: pages.round
            inset: pages.inset
        }
        WorkoutPage {
            anchors.fill: parent
            visible: pages.watch.page === "workout"
            theme: pages.theme
            watch: pages.watch
            clock: pages.clock
            round: pages.round
            inset: pages.inset
        }
        TimerPage {
            anchors.fill: parent
            visible: pages.watch.page === "timer"
            theme: pages.theme
            watch: pages.watch
            clock: pages.clock
            round: pages.round
            inset: pages.inset
        }
        StopwatchPage {
            anchors.fill: parent
            visible: pages.watch.page === "stopwatch"
            theme: pages.theme
            watch: pages.watch
            clock: pages.clock
            round: pages.round
            inset: pages.inset
        }
        MusicPage {
            anchors.fill: parent
            visible: pages.watch.page === "music"
            theme: pages.theme
            watch: pages.watch
            clock: pages.clock
            round: pages.round
            inset: pages.inset
        }
        QuickSettings {
            anchors.fill: parent
            visible: pages.watch.page === "settings"
            theme: pages.theme
            watch: pages.watch
            clock: pages.clock
            round: pages.round
            inset: pages.inset
        }
        Launcher {
            objectName: "launcher"
            anchors.fill: parent
            visible: pages.watch.page === "launcher"
            theme: pages.theme
            watch: pages.watch
            clock: pages.clock
            round: pages.round
            inset: pages.inset
        }
    }
}
