import Ljos.Ui
import Ljos.Layout
import Ljos.Core
import "logic/watch.ts" as W

// A stopwatch to a tenth of a second, with laps. It keeps counting while
// the screen sleeps, since it keeps when it started, not a count.
WatchScreen {
    id: page

    Title {
        theme: page.theme
        y: page.round ? 50 : 24
        text: t`STOPWATCH`
    }

    Text {
        id: readout
        anchors.horizontalCenter: parent.horizontalCenter
        y: page.round ? 104 : 90
        text: W.tenths(page.watch.stopwatchMs)
        color: page.watch.stopwatchRunning ? page.theme.text : page.theme.muted
        font.size: 62
        font.weight: Font.DemiBold
        font.features: { "tnum": 1 }
    }

    Column {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: readout.bottom
        anchors.topMargin: 8
        spacing: 2
        Repeater {
            // The last three laps, the newest first.
            model: W.lastLaps(page.watch.lapTimes, 3)
            delegate: Text {
                id: lapRow
                required property real modelData
                required property int index
                text: t`Lap ${page.watch.laps - lapRow.index}   ${W.tenths(lapRow.modelData)}`
                color: lapRow.index === 0 ? page.theme.cyan : page.theme.muted
                font.size: 16
                font.features: { "tnum": 1 }
            }
        }
    }

    Row {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: page.round ? 44 : 30
        spacing: 14
        Chip {
            theme: page.theme
            glyph: page.watch.stopwatchRunning ? "flag" : "close"
            label: page.watch.stopwatchRunning ? t`Lap` : t`Reset`
            onPressed: page.watch.stopwatchRunning ? page.watch.lap() : page.watch.resetStopwatch()
        }
        Chip {
            theme: page.theme
            tint: page.watch.stopwatchRunning ? page.theme.red : page.theme.green
            glyph: page.watch.stopwatchRunning ? "pause" : "play"
            help: page.watch.stopwatchRunning ? t`Stop stopwatch` : t`Start stopwatch`
            primary: true
            onPressed: page.watch.stopwatchRunning ? page.watch.stopStopwatch() : page.watch.startStopwatch()
        }
    }
}
