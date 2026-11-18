import Ljos.Ui
import Ljos.Layout
import Ljos.Core
import Ljos.Design
import "logic/watch.ts" as W

// A countdown. The crown sets its minutes; the ring round the edge runs
// down as it goes and glows; at its end the watch wakes and says so.
WatchScreen {
    id: page

    readonly property real total: page.watch.timerMinutes * 60000
    readonly property real left: page.watch.timerState === "set" ? page.total : page.watch.timerLeft
    readonly property bool ringing: page.watch.timerState === "ringing"

    Ring {
        anchors.centerIn: parent
        width: page.round ? Math.min(page.width, page.height) - 16 : Math.min(page.width - 30, page.height - 230)
        height: width
        theme: page.theme
        thickness: 10
        tint: page.ringing ? page.theme.amber : page.theme.cyan
        value: page.ringing ? 1 : page.left / Math.max(1, page.total)
        lit: page.watch.timerState === "running" || page.ringing
    }

    Title {
        theme: page.theme
        y: page.round ? 70 : 50
        text: page.ringing ? t`TIME'S UP` : t`TIMER`
    }

    Row {
        id: setter
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset: -12
        spacing: 10
        Chip {
            Layout.alignment: Align.VCenter
            visible: page.watch.timerState === "set"
            theme: page.theme
            glyph: "minus"
            help: t`One minute less`
            onPressed: page.watch.turnCrown(-1)
        }
        Text {
            id: readout
            text: page.ringing ? "00:00" : W.span(page.left)
            color: page.ringing ? page.theme.amber : page.theme.text
            font.size: page.watch.timerState === "set" ? 58 : 66
            font.weight: Font.DemiBold
            font.features: { "tnum": 1 }
            Accessible.role: Accessible.StaticText
            Accessible.name: t`${page.watch.timerMinutes} minutes`
        }
        Chip {
            Layout.alignment: Align.VCenter
            visible: page.watch.timerState === "set"
            theme: page.theme
            glyph: "plus"
            help: t`One minute more`
            onPressed: page.watch.turnCrown(1)
        }
    }
    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: setter.bottom
        anchors.topMargin: 2
        visible: page.watch.timerState === "set"
        text: t`Turn the crown to set`
        color: page.theme.muted
        font.size: 14
    }

    Row {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: page.round ? 54 : 40
        spacing: 14
        Chip {
            visible: page.watch.timerState === "running" || page.watch.timerState === "paused"
            theme: page.theme
            glyph: "close"
            help: t`Cancel timer`
            onPressed: page.watch.cancelTimer()
        }
        Chip {
            visible: page.watch.timerState === "set" || page.watch.timerState === "paused"
            theme: page.theme
            glyph: "play"
            label: page.watch.timerState === "set" ? t`Start` : t`Resume`
            help: page.watch.timerState === "set" ? t`Start timer` : t`Resume timer`
            primary: true
            onPressed: page.watch.startTimer()
        }
        Chip {
            visible: page.watch.timerState === "running"
            theme: page.theme
            glyph: "pause"
            help: t`Pause timer`
            onPressed: page.watch.pauseTimer()
        }
        Chip {
            visible: page.ringing
            theme: page.theme
            tint: page.theme.amber
            glyph: "check"
            label: t`Stop`
            primary: true
            onPressed: page.watch.cancelTimer()
        }
    }
}
