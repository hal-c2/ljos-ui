import Ljos.Ui
import Ljos.Core
import Ljos.Controls
import Ljos.Layout
import "logic/inbox.ts" as Inbox

// Records created per week over the last twelve, for what the list shows
// (so a search changes the chart), and the counts by state and priority.
// Drawn with rectangles: a bar chart needs nothing more.
Item {
    id: page
    required property Store inbox
    // Counted only while the tab shows: hidden, it would count again on
    // every keystroke of a search.
    readonly property bool shown: page.inbox.tab === 2
    readonly property list<Inbox.Week> weeks: page.shown ? Inbox.weekly(page.inbox.matches) : []
    readonly property int scaleTop: Inbox.niceMax(page.weeks)
    readonly property Inbox.Tally tally: page.shown ? Inbox.tally(page.inbox.matches) : Inbox.tally([])

    readonly property int chartX: 56
    readonly property int chartY: 64
    readonly property int chartW: 480
    readonly property int chartH: 260
    readonly property real slot: page.chartW / 12

    Text {
        x: 20
        y: 16
        text: t`Created per week`
        color: Theme.text
        font.size: 16
        font.weight: Font.DemiBold
    }

    // The scale: its top and the baseline.
    Text {
        elide: Text.ElideRight
        x: 0
        y: page.chartY - 8
        width: page.chartX - 8
        align: Text.AlignRight
        text: page.inbox.count(page.scaleTop)
        color: Theme.textMuted
        font.size: 11
    }

    Rectangle {
        x: page.chartX
        y: page.chartY
        width: page.chartW
        height: 1
        fill: Theme.border
    }

    Rectangle {
        x: page.chartX
        y: page.chartY + page.chartH
        width: page.chartW
        height: 1
        fill: Theme.textMuted
    }

    Repeater {
        model: page.weeks
        delegate: Item {
            id: bar
            required property Inbox.Week modelData
            required property int index
            readonly property real h: page.scaleTop > 0 ? page.chartH * modelData.count / page.scaleTop : 0

            x: page.chartX + bar.index * page.slot
            y: page.chartY
            width: page.slot
            height: page.chartH + 40
            Accessible.role: Accessible.StaticText
            Accessible.name: {
                const week = page.inbox.week(bar.modelData.start)
                const n = bar.modelData.count
                return plural(n, { one: `${week}: # issue`, other: `${week}: # issues` })
            }

            Rectangle {
                x: 6
                y: page.chartH - bar.h
                width: bar.width - 12
                height: bar.h
                radius: 3
                fill: Theme.accent
            }

            Text {
                elide: Text.ElideRight
                x: 0
                y: page.chartH - bar.h - 18
                width: bar.width
                align: Text.AlignHCenter
                text: page.inbox.count(bar.modelData.count)
                color: Theme.text
                font.size: 11
            }

            Text {
                elide: Text.ElideRight
                x: 0
                y: page.chartH + 6
                width: bar.width
                align: Text.AlignHCenter
                text: page.inbox.week(bar.modelData.start)
                color: Theme.textMuted
                font.size: 10
            }
        }
    }

    Column {
        x: 20
        y: page.chartY + page.chartH + 56
        spacing: 6

        Text {
            text: {
                const open = page.inbox.count(page.tally.open)
                const closed = page.inbox.count(page.tally.closed)
                return t`Open: ${open} · Closed: ${closed}`
            }
            color: Theme.text
            font.size: 13
        }

        Text {
            text: {
                const by = page.tally.byPriority
                const urgent = page.inbox.count(by[0] ?? 0)
                const high = page.inbox.count(by[1] ?? 0)
                const normal = page.inbox.count(by[2] ?? 0)
                const low = page.inbox.count(by[3] ?? 0)
                return t`Urgent: ${urgent} · High: ${high} · Normal: ${normal} · Low: ${low}`
            }
            color: Theme.text
            font.size: 13
        }
    }
}
