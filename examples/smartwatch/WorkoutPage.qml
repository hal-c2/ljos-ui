import Ljos.Ui
import Ljos.Layout
import Ljos.Core
import "logic/watch.ts" as W
import "logic/health.ts" as Health

// An outdoor run: start it, watch the time, the heart and the distance,
// pause it, end it, and read what it came to.
WatchScreen {
    id: page

    readonly property bool going: page.watch.workout === "running" || page.watch.workout === "paused"

    Title {
        theme: page.theme
        y: page.round ? 50 : 24
        text: page.watch.workout === "done" ? t`RUN COMPLETE` : page.watch.workout === "paused" ? t`PAUSED` : t`OUTDOOR RUN`
    }

    // Before: the run, and Start.
    Column {
        anchors.centerIn: parent
        spacing: 18
        visible: page.watch.workout === "idle"
        Neon {
            Layout.alignment: Align.HCenter
            width: 96
            height: 96
            theme: page.theme
            tint: page.theme.green
            spread: 0.6
            Rectangle {
                anchors.fill: parent
                radius: 48
                fill: "transparent"
                border.color: page.theme.green
                border.width: 3
            }
            Icon {
                anchors.centerIn: parent
                width: 50
                height: 50
                glyph: "run"
                color: page.theme.green
            }
        }
        Chip {
            Layout.alignment: Align.HCenter
            theme: page.theme
            tint: page.theme.green
            glyph: "play"
            label: t`Start run`
            primary: true
            onPressed: page.watch.startWorkout()
        }
    }

    // During: time, heart, distance.
    Item {
        anchors.fill: parent
        visible: page.going || page.watch.workout === "done"
        Text {
            id: elapsed
            anchors.horizontalCenter: parent.horizontalCenter
            y: page.round ? 84 : 66
            text: W.span(page.watch.workoutSeconds * 1000)
            color: page.watch.workout === "paused" ? page.theme.amber : page.theme.text
            font.size: 60
            font.weight: Font.DemiBold
            font.features: { "tnum": 1 }
        }
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.top: elapsed.bottom
            anchors.topMargin: 8
            spacing: 26
            Column {
                Text {
                    Layout.alignment: Align.HCenter
                    text: `${page.watch.bpm}`
                    color: page.theme.magenta
                    font.size: 28
                    font.weight: Font.DemiBold
                    font.features: { "tnum": 1 }
                }
                Text {
                    Layout.alignment: Align.HCenter
                    text: t`BPM`
                    color: page.theme.muted
                    font.size: 13
                    font.letterSpacing: 1.5
                }
            }
            Column {
                Text {
                    Layout.alignment: Align.HCenter
                    text: Health.km(page.watch.workoutSeconds).toFixed(2)
                    color: page.theme.cyan
                    font.size: 28
                    font.weight: Font.DemiBold
                    font.features: { "tnum": 1 }
                }
                Text {
                    Layout.alignment: Align.HCenter
                    text: t`KM`
                    color: page.theme.muted
                    font.size: 13
                    font.letterSpacing: 1.5
                }
            }
            Column {
                Text {
                    Layout.alignment: Align.HCenter
                    text: `${Health.kcal(page.watch.workoutSeconds)}`
                    color: page.theme.amber
                    font.size: 28
                    font.weight: Font.DemiBold
                    font.features: { "tnum": 1 }
                }
                Text {
                    Layout.alignment: Align.HCenter
                    text: t`KCAL`
                    color: page.theme.muted
                    font.size: 13
                    font.letterSpacing: 1.5
                }
            }
        }
    }

    Row {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: page.round ? 44 : 30
        spacing: 14
        visible: page.going
        Chip {
            visible: page.watch.workout === "paused"
            theme: page.theme
            tint: page.theme.red
            glyph: "flag"
            label: t`End`
            onPressed: page.watch.endWorkout()
        }
        Chip {
            visible: page.watch.workout === "running"
            theme: page.theme
            glyph: "pause"
            label: t`Pause`
            onPressed: page.watch.pauseWorkout()
        }
        Chip {
            visible: page.watch.workout === "paused"
            theme: page.theme
            tint: page.theme.green
            glyph: "play"
            label: t`Resume`
            primary: true
            onPressed: page.watch.resumeWorkout()
        }
    }
    Chip {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: page.round ? 44 : 30
        visible: page.watch.workout === "done"
        theme: page.theme
        glyph: "check"
        label: t`Done`
        primary: true
        onPressed: page.watch.closeWorkout()
    }
}
