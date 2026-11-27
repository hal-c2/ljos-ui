import Ljos.Ui
import Ljos.Core
import Ljos.Controls
import Ljos.Layout
import "logic/inbox.ts" as Inbox

// One record in the list: who opened it and when, its title, its priority,
// up to three labels, a star and the number of comments.
Rectangle {
    id: row
    required property Inbox.Key modelData
    required property int index
    required property Store inbox
    signal picked()

    readonly property Inbox.Issue issue: row.inbox.issue(row.modelData)
    readonly property bool current: ListView.isCurrentItem

    height: 64
    fill: row.current ? Theme.accentSurface : hover.hovered ? Theme.surface : Theme.canvas
    Accessible.role: Accessible.ListItem
    Accessible.name: row.issue.title
    Accessible.description: {
        const position = row.index + 1
        const total = row.inbox.rows.length
        return t`${position} of ${total}`
    }
    Accessible.selected: row.current

    Behavior on fill {
        ColorAnimation { duration: 100 }
    }

    // The priority as a bar at the row's edge: urgent and high stand out.
    Rectangle {
        x: 0
        y: 10
        width: 3
        height: 44
        radius: 1.5
        fill: row.inbox.priorityColor(row.issue.priority)
    }

    Avatar {
        x: 16
        y: 16
        size: 32
        source: row.issue.avatar
    }

    Text {
        x: 60
        y: 12
        width: row.width - 60 - 64
        text: row.issue.title
        color: Theme.text
        font.size: 14
        font.weight: Font.Medium
        elide: Text.ElideRight
    }

    Row {
        x: 60
        y: 36
        width: row.width - 60 - 64
        spacing: 6

        Text {
            text: `${row.issue.key} · ${row.issue.author} · ${row.inbox.ago(row.issue.updated)}`
            color: Theme.textMuted
            font.size: 12
            elide: Text.ElideRight
            width: Math.min(implicitWidth, 250)
        }

        Repeater {
            model: row.issue.tags.slice(0, 3)
            delegate: Chip {
                required property string modelData
                text: modelData
            }
        }
    }

    Icon {
        x: row.width - 52
        y: 12
        visible: row.issue.starred
        name: "star"
        filled: true
        size: 14
        color: Theme.warningForeground
        Accessible.name: t`Starred`
    }

    Text {
        elide: Text.ElideRight
        x: row.width - 56
        y: 36
        width: 40
        align: Text.AlignRight
        visible: row.issue.comments > 0
        text: row.inbox.count(row.issue.comments)
        color: Theme.textMuted
        font.size: 12
    }

    Hover {
        id: hover
    }

    Tap {
        onTapped: row.picked()
    }
}
