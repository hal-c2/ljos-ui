import Ljos.Ui
import Ljos.Core
import "logic/watch.ts" as W
import "logic/health.ts" as Health
import "logic/media.ts" as Media
import "logic/notes.ts" as Notes

// Everything the watch is. The screens only read it and call its
// functions, so a test (or an agent) wears the watch through the same
// doors a finger, the crown and the side button do.
//
// Time that goes on while the screen sleeps (a workout, the stopwatch, a
// track) is kept as when it started and how much was banked before, never
// as a count of ticks: the watch need not wake to keep it.
Object {
    id: watch

    required property WallClock clock

    // The wall clock's reading, typed for the faces.
    readonly property int hours: watch.clock.hours
    readonly property int minutes: watch.clock.minutes
    readonly property int seconds: watch.clock.seconds
    readonly property int weekday: watch.clock.weekday
    readonly property int day: watch.clock.day
    readonly property int month: watch.clock.month

    // ----- Screens ----------------------------------------------------------

    property W.Page page: "face"
    // The face shown on "face": 0 Pulse, 1 Grid.
    property int face: 0
    readonly property int faces: 2
    // Asleep: the always-on face, or a dark screen with always on off.
    property bool ambient: false
    // The highlighted row of a list, moved by the crown.
    property int choice: 0
    // Bumped by every touch, turn and press, so the idle timer starts over.
    signal touched()

    function show(to: W.Page) {
        watch.page = to
        watch.choice = 0
        watch.refresh()
    }

    function swiped(dx: real, dy: real) {
        watch.touch()
        const to = W.swipeTo(watch.page, W.swipe(dx, dy))
        if (to !== watch.page) {
            watch.show(to)
        }
    }

    function back() {
        watch.touch()
        watch.show(W.back(watch.page))
    }

    // The side button: wake, and come home.
    function pressSide() {
        watch.touch()
        watch.show("face")
    }

    function nextFace() {
        watch.touch()
        watch.face = (watch.face + 1) % watch.faces
    }

    function touch() {
        if (watch.ambient) {
            watch.ambient = false
            watch.refresh()
        }
        watch.touched()
    }

    function sleep() {
        watch.ambient = true
        if (watch.page === "notification") {
            watch.page = "notifications"
        }
    }

    // The crown: one notch is one step, down is towards the end.
    function turnCrown(steps: int) {
        watch.touch()
        if (watch.page === "launcher") {
            watch.choice = W.clamp(watch.choice + steps, 0, W.apps.length - 1)
        } else if (watch.page === "notifications") {
            watch.choice = W.clamp(watch.choice + steps, 0, Math.max(0, watch.unread - 1))
        } else if (watch.page === "timer" && watch.timerState === "set") {
            watch.timerMinutes = W.clamp(watch.timerMinutes + steps, 1, 60)
        } else if (watch.page === "music") {
            // The crown turned up is louder.
            watch.volume = W.clamp(watch.volume - steps, 0, 10)
        }
    }

    // Return, or the crown pressed: open what the crown points at.
    function select() {
        watch.touch()
        if (watch.page === "launcher") {
            const app = W.apps[watch.choice]
            if (app !== undefined) {
                watch.show(app.page)
            }
        } else if (watch.page === "notifications" && watch.unread > 0) {
            const note = watch.inbox[watch.choice]
            if (note !== undefined) {
                watch.open(note.id)
            }
        }
    }

    // ----- Notifications ------------------------------------------------------

    property list<int> gone: []
    property int opened: 0
    readonly property list<Notes.Note> inbox: Notes.unread(watch.gone)
    readonly property int unread: watch.inbox.length
    readonly property Notes.Note note: Notes.byId(watch.opened)

    function open(id: int) {
        watch.touch()
        watch.opened = id
        watch.page = "notification"
    }

    function dismiss(id: int) {
        watch.touch()
        watch.gone = watch.gone.concat([id])
        watch.show("notifications")
    }

    function clearAll() {
        watch.touch()
        watch.gone = Notes.all.map((n: Notes.Note) => n.id)
    }

    function noteTitle(kind: Notes.Kind): string {
        return kind === "message" ? t`Are we still on for tonight?`
            : kind === "calendar" ? t`Design review at 10:00`
            : kind === "coach" ? t`You are 2,100 steps from your goal`
            : t`Battery at 20 %`
    }

    function noteBody(kind: Notes.Kind): string {
        return kind === "message" ? t`The rooftop place opens at eight. Bring the jacket, it gets cold up there.`
            : kind === "calendar" ? t`Room Hekla, with the hardware team. The prototype boards came in this morning.`
            : kind === "coach" ? t`A ten-minute walk after lunch would close your move ring.`
            : t`Always on stays on. Glow turns off below 10 % to save power.`
    }

    function ago(minutes: int): string {
        return minutes < 60 ? t`${minutes} min ago` : t`1 h ago`
    }

    // ----- Heart and activity -------------------------------------------------

    property int bpm: 66
    property int samples: 0
    property list<real> history: Health.seed(40)
    readonly property int historyKeep: 40
    property int steps: 7900
    readonly property Health.Rings rings: Health.rings(watch.steps, watch.workoutSeconds)

    function sample() {
        watch.samples = watch.samples + 1
        watch.bpm = Health.bpm(watch.samples, watch.workout === "running", watch.workoutSeconds)
        watch.history = Health.push(watch.history, watch.bpm, watch.historyKeep)
        if (watch.workout === "running") {
            watch.steps = watch.steps + 3
        }
    }

    // ----- Workout --------------------------------------------------------------

    property W.Workout workout: "idle"
    property real workoutFrom: 0
    property real workoutBanked: 0
    property int workoutSeconds: 0

    function startWorkout() {
        watch.touch()
        watch.workoutBanked = 0
        watch.workoutFrom = Date.now()
        watch.workout = "running"
        watch.workoutSeconds = 0
    }

    function pauseWorkout() {
        watch.touch()
        watch.workoutBanked = watch.workoutBanked + Date.now() - watch.workoutFrom
        watch.workout = "paused"
        watch.refresh()
    }

    function resumeWorkout() {
        watch.touch()
        watch.workoutFrom = Date.now()
        watch.workout = "running"
    }

    function endWorkout() {
        watch.touch()
        if (watch.workout === "running") {
            watch.pauseWorkout()
        }
        watch.workout = "done"
    }

    function closeWorkout() {
        watch.touch()
        watch.workout = "idle"
        watch.workoutSeconds = 0
    }

    // ----- Timer and stopwatch ----------------------------------------------------

    property int timerMinutes: 5
    property W.Countdown timerState: "set"
    property real timerEnds: 0
    property real timerLeft: 0
    // The one wake a countdown needs: its end. Main.qml's "ring" timer
    // follows these two.
    readonly property bool counting: watch.timerState === "running"
    readonly property real timerDue: watch.timerState === "running" ? watch.timerEnds : 0

    function startTimer() {
        watch.touch()
        const left = watch.timerState === "paused" ? watch.timerLeft : watch.timerMinutes * 60000
        watch.timerEnds = Date.now() + left
        watch.timerLeft = left
        watch.timerState = "running"
    }

    function pauseTimer() {
        watch.touch()
        watch.timerLeft = Math.max(0, watch.timerEnds - Date.now())
        watch.timerState = "paused"
    }

    function cancelTimer() {
        watch.touch()
        watch.timerState = "set"
        watch.timerLeft = 0
    }

    function ring() {
        watch.timerLeft = 0
        watch.timerState = "ringing"
        // An alarm wakes the screen, and shows itself.
        watch.touch()
        watch.page = "timer"
    }

    property bool stopwatchRunning: false
    property real stopwatchFrom: 0
    property real stopwatchBanked: 0
    property real stopwatchMs: 0
    property list<real> lapTimes: []
    readonly property int laps: watch.lapTimes.length

    function startStopwatch() {
        watch.touch()
        watch.stopwatchFrom = Date.now()
        watch.stopwatchRunning = true
    }

    function stopStopwatch() {
        watch.touch()
        watch.stopwatchBanked = watch.stopwatchBanked + Date.now() - watch.stopwatchFrom
        watch.stopwatchMs = watch.stopwatchBanked
        watch.stopwatchRunning = false
    }

    function lap() {
        watch.touch()
        watch.refresh()
        watch.lapTimes = watch.lapTimes.concat([watch.stopwatchMs])
    }

    function resetStopwatch() {
        watch.touch()
        watch.stopwatchRunning = false
        watch.stopwatchBanked = 0
        watch.stopwatchMs = 0
        watch.lapTimes = []
    }

    // ----- Music --------------------------------------------------------------------

    property bool playing: false
    property int track: 0
    property real playFrom: 0
    property real playBanked: 0
    // Seconds into the track.
    property real position: 0
    property int volume: 6
    readonly property Media.Track nowPlaying: Media.at(watch.track)

    function play() {
        watch.touch()
        watch.playFrom = Date.now()
        watch.playing = true
    }

    function pause() {
        watch.touch()
        watch.playBanked = watch.playBanked + Date.now() - watch.playFrom
        watch.playing = false
        watch.refresh()
    }

    function skip(by: int) {
        watch.touch()
        watch.track = (watch.track + by + Media.tracks.length) % Media.tracks.length
        watch.playBanked = 0
        watch.playFrom = Date.now()
        watch.position = 0
    }

    // ----- Quick settings -------------------------------------------------------------

    property bool quiet: false
    property bool alwaysOn: true
    property bool bloom: true
    property int battery: 82

    // ----- Keeping up -------------------------------------------------------------------

    // Bring every running count up to now: called each second while a
    // screen shows one, and once on waking.
    function refresh() {
        const now = Date.now()
        if (watch.workout === "running") {
            watch.workoutSeconds = Math.floor((watch.workoutBanked + now - watch.workoutFrom) / 1000)
        } else {
            watch.workoutSeconds = Math.floor(watch.workoutBanked / 1000)
        }
        if (watch.stopwatchRunning) {
            watch.stopwatchMs = watch.stopwatchBanked + now - watch.stopwatchFrom
        }
        if (watch.timerState === "running") {
            watch.timerLeft = Math.max(0, watch.timerEnds - now)
        }
        if (watch.playing) {
            let at = (watch.playBanked + now - watch.playFrom) / 1000
            while (at >= watch.nowPlaying.seconds) {
                at = at - watch.nowPlaying.seconds
                watch.track = (watch.track + 1) % Media.tracks.length
                watch.playBanked = 0
                watch.playFrom = now - at * 1000
            }
            watch.position = at
        } else {
            watch.position = watch.playBanked / 1000
        }
    }

    // Whether the screen shows something that changes each second.
    readonly property bool ticking: !watch.ambient && (watch.page === "face"
        || (watch.page === "workout" && watch.workout === "running")
        || (watch.page === "timer" && watch.timerState === "running")
        || (watch.page === "stopwatch" && watch.stopwatchRunning)
        || (watch.page === "music" && watch.playing))
    // A stopwatch shows tenths.
    readonly property int tickEvery: watch.page === "stopwatch" ? 100 : 1000
    readonly property bool sampling: !watch.ambient
        && (watch.page === "health" || (watch.page === "workout" && watch.workout === "running"))

    // ----- Words ------------------------------------------------------------------------------

    function title(of: W.Page): string {
        return of === "health" ? t`Heart`
            : of === "workout" ? t`Workout`
            : of === "timer" ? t`Timer`
            : of === "stopwatch" ? t`Stopwatch`
            : of === "music" ? t`Music`
            : of === "settings" ? t`Settings`
            : of === "notifications" ? t`Notifications`
            : of === "notification" ? t`Message`
            : of === "launcher" ? t`Apps`
            : t`Face`
    }

    function weekdayName(d: int): string {
        return d === 0 ? t`SUN` : d === 1 ? t`MON` : d === 2 ? t`TUE` : d === 3 ? t`WED`
            : d === 4 ? t`THU` : d === 5 ? t`FRI` : t`SAT`
    }

    function monthName(m: int): string {
        return m === 0 ? t`JAN` : m === 1 ? t`FEB` : m === 2 ? t`MAR` : m === 3 ? t`APR`
            : m === 4 ? t`MAY` : m === 5 ? t`JUN` : m === 6 ? t`JUL` : m === 7 ? t`AUG`
            : m === 8 ? t`SEP` : m === 9 ? t`OCT` : m === 10 ? t`NOV` : t`DEC`
    }
}
