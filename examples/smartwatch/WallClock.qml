import QtQuick

// The time of day. Ljós's own clock source is not built yet, and
// DIALECT.md keeps a file that needs Date's local-time getters in QML, so
// this one stays QML. By itself it wakes once a minute, on the minute;
// while someone looks at a second hand the watch calls `read()` each
// second as well.
QtObject {
    id: clock

    property int hours: 0
    property int minutes: 0
    property int seconds: 0
    // 0 is Sunday; 0 is January.
    property int weekday: 0
    property int day: 1
    property int month: 0

    // Milliseconds since the epoch, for what counts time across a sleep.
    function now(): real {
        return Date.now()
    }

    function read() {
        clock.publish(new Date())
    }

    function publish(at) {
        clock.hours = at.getHours()
        clock.minutes = at.getMinutes()
        clock.seconds = at.getSeconds()
        clock.weekday = at.getDay()
        clock.day = at.getDate()
        clock.month = at.getMonth()
    }

    // One reading for both the face and the next wake: two could fall
    // either side of a minute, and the face would keep the old one for a
    // whole minute more.
    function show() {
        const at = new Date()
        clock.publish(at)
        turn.interval = Math.max(1, 60000 - at.getSeconds() * 1000 - at.getMilliseconds())
        turn.restart()
    }

    property Timer turn: Timer {
        interval: 60000
        onTriggered: clock.show()
    }

    Component.onCompleted: clock.show()
}
