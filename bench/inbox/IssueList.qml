import Ljos.Ui
import Ljos.Core
import Ljos.Controls
import "logic/inbox.ts" as Inbox

// The list: a header with the count and the sort, then one 64 px row per
// record the view shows. Only the rows in sight exist; the list is keyed by
// id, so a sort or a search keeps each row that stays and animates only
// those that come and go.
Rectangle {
    id: pane
    required property Store inbox
    fill: Theme.canvas

    function focusList(): void {
        view.forceActiveFocus()
    }

    Text {
        elide: Text.ElideRight
        x: 16
        y: 0
        width: pane.width - 32
        height: 32
        verticalAlignment: Text.AlignVCenter
        text: {
            const n = pane.inbox.rows.length
            return plural(n, { one: "# issue", other: "# issues" })
        }
        Accessible.role: Accessible.StaticText
        Accessible.name: text
        color: Theme.textMuted
        font.size: 12
        font.weight: Font.DemiBold
    }

    Text {
        elide: Text.ElideRight
        x: 16
        y: 0
        width: pane.width - 32
        height: 32
        align: Text.AlignRight
        verticalAlignment: Text.AlignVCenter
        text: pane.inbox.sortNames()[Inbox.sorts.indexOf(pane.inbox.sort)] ?? ""
        color: Theme.textMuted
        font.size: 12
    }

    ListView {
        id: view
        objectName: "issues"
        x: 0
        y: 32
        width: pane.width
        height: pane.height - 32
        clip: true
        focus: true
        model: pane.inbox.rows
        key: "id"
        currentIndex: pane.inbox.selectedIndex
        keyNavigationEnabled: false
        boundsBehavior: Flickable.StopAtBounds
        Accessible.role: Accessible.List
        Accessible.name: t`Issues`

        onCurrentIndexChanged: {
            if (view.currentIndex >= 0) {
                view.positionViewAtIndex(view.currentIndex, ListView.Contain)
            }
        }

        Keys.onPressed: (event: KeyEvent) => {
            const i = pane.inbox.selectedIndex
            const n = pane.inbox.rows.length
            if (event.key === Key.Down) {
                pane.inbox.selectAt(i + 1)
            } else if (event.key === Key.Up) {
                pane.inbox.selectAt(i < 0 ? 0 : i - 1)
            } else if (event.key === Key.PageDown) {
                pane.inbox.selectAt(i + 10)
            } else if (event.key === Key.PageUp) {
                pane.inbox.selectAt(i < 0 ? 0 : i - 10)
            } else if (event.key === Key.Home) {
                pane.inbox.selectAt(0)
            } else if (event.key === Key.End) {
                pane.inbox.selectAt(n - 1)
            } else if (event.key === Key.Return || event.key === Key.Enter) {
                pane.inbox.tab = 0
            } else {
                return
            }
            event.accepted = true
        }

        delegate: IssueRow {
            width: ListView.view.width
            inbox: pane.inbox
            onPicked: {
                pane.inbox.select(modelData.id)
                view.forceActiveFocus()
            }
        }

        // A new row grows into its place and fades in; one removed fades
        // and the rows below close the gap. With reduced motion only the
        // fades stay, at the same length, and the rows move at once.
        enter: Transition {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 200; easing.type: Easing.OutCubic }
        }
        exit: Transition {
            NumberAnimation { property: "opacity"; to: 0; duration: 200; easing.type: Easing.OutCubic }
        }
        displaced: Transition {
            NumberAnimation { property: "y"; duration: pane.inbox.reduced ? 0 : 200; easing.type: Easing.OutCubic }
        }

        ScrollBar.vertical: ScrollBar {}
    }

    Text {
        anchors.centerIn: view
        visible: pane.inbox.rows.length === 0 && pane.inbox.all.length > 0
        text: t`No issues match`
        Accessible.role: Accessible.StaticText
        Accessible.name: text
        color: Theme.textMuted
        font.size: 14
    }
}
