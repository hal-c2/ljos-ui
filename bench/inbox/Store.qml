import Ljos.Ui
import Ljos.Core
import Ljos.Controls
import Ljos.Design
import "logic/inbox.ts" as Inbox

// What the inbox holds and what it is showing, and the changes the views
// ask for. The views are handed this object and read it; none of them
// keeps a copy of the records.
Object {
    id: store

    // Every record, as loaded and as edited since; edits live in memory.
    property list<Inbox.Issue> all: []
    property Inbox.Sort sort: "updated"
    property Inbox.Filter filter: "inbox"
    property string query: ""
    property int selectedId: -1
    // 0 Conversation, 1 Edit, 2 Insights.
    property int tab: 0
    // "light", "dark" or "system".
    property string look: "system"
    // Starts as the system's setting; the settings dialog changes it.
    property bool reduced: Motion.reduced

    readonly property bool dark: store.look === "dark"
        || (store.look === "system" && App.hints.colorScheme === ColorScheme.Dark)
    // The order is worked out when the records or the sort change; a key
    // typed in the search only filters it.
    readonly property list<Inbox.Issue> ordered: Inbox.sorted(store.all, store.sort)
    readonly property Inbox.Shown shown: Inbox.shown(store.ordered, store.filter, store.query)
    readonly property list<Inbox.Issue> matches: store.shown.issues
    readonly property list<Inbox.Key> rows: store.shown.keys
    readonly property int selectedIndex: Inbox.indexOf(store.rows, store.selectedId)
    readonly property Inbox.Issue selected: store.selectedId <= 0 ? Inbox.none : Inbox.pick(store.all, store.selectedId)
    // Gathered a turn after the records arrive (`loaded`), and the labels
    // again after a save, which may add one: the first turn with ten
    // thousand records in it has the list to make and no time to spare.
    property list<string> tags: []
    property list<string> people: []

    onDarkChanged: Colors.dark = store.dark
    Component.onCompleted: Colors.dark = store.dark

    // The choices' names, made in a function: a list of translated texts
    // written into a binding keeps the language it was first read in, and
    // one a function returns follows the language (an engine bug, written
    // up in bench/inbox/README.md).
    function sortNames(): list<string> {
        return [t`Updated`, t`Created`, t`Priority`, t`Title`]
    }

    function priorityNames(): list<string> {
        return [t`Urgent`, t`High`, t`Normal`, t`Low`]
    }

    function assignees(): list<string> {
        return [t`Nobody`].concat(store.people)
    }

    function issue(key: Inbox.Key): Inbox.Issue {
        return Inbox.at(store.shown, key, store.all)
    }

    function select(id: number): void {
        store.selectedId = id
    }

    function selectAt(i: number): void {
        if (store.rows.length > 0) {
            store.selectedId = store.rows[Inbox.clamp(i, store.rows.length)]?.id ?? -1
        }
    }

    function loaded(rows: readonly Inbox.Issue[]): void {
        store.all = rows
        later(() => {
            store.tags = Inbox.tagsOf(rows)
        })
        later(() => {
            store.people = Inbox.peopleOf(rows)
        })
    }

    function save(title: string, tags: string, priority: number, assignee: string, body: string): void {
        store.all = Inbox.saved(store.all, store.selectedId, title, tags, priority, assignee, body)
        store.tags = Inbox.tagsOf(store.all)
        store.tab = 0
    }

    function toggleStatus(): void {
        const id = store.selectedId
        if (store.selected.status === "open") {
            const next = Inbox.after(store.rows, id)
            store.all = Inbox.withStatus(store.all, id, "closed")
            if (Inbox.indexOf(store.rows, id) < 0) {
                store.selectedId = next
            }
        } else {
            store.all = Inbox.withStatus(store.all, id, "open")
        }
    }

    // Urgent, high, normal, low.
    function priorityColor(p: number): color {
        return p === 0 ? Theme.destructive
            : p === 1 ? Theme.warningForeground
            : p === 2 ? Theme.accent
            : Theme.iconMuted
    }

    function ago(ms: number): string {
        const { unit, n } = Inbox.ago(ms)
        return unit === "now" ? t`just now`
            : unit === "hours" ? plural(n, { one: "# hour ago", other: "# hours ago" })
            : plural(n, { one: "# day ago", other: "# days ago" })
    }

    function day(ms: number): string {
        const { year, month, day } = Inbox.dayOf(ms)
        return t`${month}/${day}/${year}`
    }

    // A count as the language writes it (grouped: 10,000). The locale's
    // own number formatting is not typed for scripts yet, and a plural's
    // `#` is formatted, so a form with only `other` is the number.
    function count(n: number): string {
        return plural(n, { other: "#" })
    }

    function week(ms: number): string {
        const { month, day } = Inbox.dayOf(ms)
        return t`${month}/${day}`
    }

    function create(): void {
        const id = Inbox.nextId(store.all)
        store.all = Inbox.created(store.all, t`Untitled issue`)
        store.selectedId = id
        store.tab = 1
    }
}
