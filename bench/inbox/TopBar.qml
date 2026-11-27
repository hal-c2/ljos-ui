import Ljos.Ui
import Ljos.Core
import Ljos.Controls
import "logic/inbox.ts" as Inbox

// The bar across the top: the app's name, the search, the sort, a new
// issue and the theme. Each control sits where the layout pins it.
Rectangle {
    id: bar
    // Down in the search moves on to the list.
    signal down()
    required property Store inbox
    fill: Theme.surface

    Text {
        x: 16
        y: 0
        width: 188
        height: 56
        verticalAlignment: Text.AlignVCenter
        text: t`Inbox`
        color: Theme.text
        font.size: 17
        font.weight: Font.DemiBold
        elide: Text.ElideRight
    }

    TextField {
        id: search
        objectName: "search"
        x: 236
        y: 12
        width: 400
        height: 32
        placeholderText: t`Search issues`
        Accessible.name: t`Search issues`
        onTextChanged: bar.inbox.query = search.text
        Keys.onPressed: (event: KeyEvent) => {
            if (event.key === Key.Escape) {
                search.text = ""
                event.accepted = true
            } else if (event.key === Key.Down) {
                bar.down()
                event.accepted = true
            }
        }
    }

    ComboBox {
        id: sort
        objectName: "sort"
        x: 672
        y: 12
        width: 120
        height: 32
        Accessible.name: t`Sort`
        model: bar.inbox.sortNames()
        onActivated: (index: number) => {
            bar.inbox.sort = Inbox.sorts[index]
        }
    }

    Button {
        objectName: "new"
        x: 818
        y: 12
        width: 140
        height: 32
        primary: true
        iconName: "plus"
        text: t`New issue`
        enabled: bar.inbox.ready
        onClicked: bar.inbox.create()
    }

    ToolButton {
        objectName: "theme"
        x: 1226
        y: 12
        width: 32
        height: 32
        iconName: bar.inbox.dark ? "sun" : "moon"
        Accessible.name: bar.inbox.dark ? t`Light` : t`Dark`
        onClicked: bar.inbox.look = bar.inbox.dark ? "light" : "dark"
    }
}
