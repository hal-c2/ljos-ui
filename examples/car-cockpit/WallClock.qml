import QtQuick

// The time of day for the status bar. Ljós's own clock source is not built
// yet, and DIALECT.md keeps a file that needs Date's local-time getters in
// QML, so this one stays QML: it wakes once a minute, on the minute.
QtObject {
    id: clock

    // "9:41", as hours and minutes of local time.
    property string text: ""

    function show() {
        const now = new Date()
        const minutes = now.getMinutes()
        clock.text = now.getHours() + ":" + (minutes < 10 ? "0" : "") + minutes
        turn.interval = Math.max(1, 60000 - now.getSeconds() * 1000 - now.getMilliseconds())
        turn.restart()
    }

    property Timer turn: Timer {
        interval: 60000
        onTriggered: clock.show()
    }

    Component.onCompleted: clock.show()
}
