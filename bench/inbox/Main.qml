import Ljos.Ui
import Ljos.Core
import Ljos.Controls
import Ljos.Models
import "logic/inbox.ts" as Inbox

// A team's issue inbox: ten thousand reports, a list that is filtered,
// sorted and searched as you type, a detail pane with a form that refuses
// bad input, a chart, a settings dialog, two themes and two languages.
// The layout is pinned in logical pixels (bench/inbox/README.md), so one
// script clicks the same control in every toolkit it is written in.
Rectangle {
    id: root
    width: 1280
    height: 800
    fill: Theme.canvas

    Store {
        id: store
        objectName: "store"
    }

    JsonListModel {
        id: source
        source: "data/issues.json"
        onStatusChanged: {
            if (source.status === JsonListModel.Ready) {
                root.load(0, [])
            }
        }
    }

    // Ten thousand rows take longer to read than one turn of script may
    // run, so they are read a thousand at a time, a turn each; the list
    // appears whole once the last is read.
    function load(from: number, out: Inbox.Issue[]): void {
        const to = Math.min(source.count, from + 1000)
        for (let i = from; i < to; i += 1) {
            out.push(Inbox.decode(source.get(i)))
        }
        if (to < source.count) {
            later(() => root.load(to, out))
        } else {
            store.loaded(out)
        }
    }

    TopBar {
        id: top
        x: 0
        y: 0
        width: root.width
        height: 56
        inbox: store
        onDown: list.focusList()
    }

    Sidebar {
        x: 0
        y: 56
        width: 220
        height: root.height - 56
        inbox: store
        onOpenSettings: settings.open()
    }

    IssueList {
        id: list
        x: 220
        y: 56
        width: 480
        height: root.height - 56
        inbox: store
    }

    Detail {
        x: 700
        y: 56
        width: root.width - 700
        height: root.height - 56
        inbox: store
    }

    Rectangle {
        x: 220
        y: 0
        width: 1
        height: root.height
        fill: Theme.border
    }

    Rectangle {
        x: 700
        y: 56
        width: 1
        height: root.height - 56
        fill: Theme.border
    }

    Rectangle {
        x: 0
        y: 56
        width: root.width
        height: 1
        fill: Theme.border
    }

    SettingsDialog {
        id: settings
        inbox: store
    }
}
