import Ljos.Ui
import Ljos.Core
import Ljos.Controls

// The selected record, in three tabs. A tab's panel slides in from its
// tab's side and fades in; with reduced motion it only fades.
Rectangle {
    id: detail
    required property Store inbox
    fill: Theme.canvas

    TabBar {
        id: tabs
        objectName: "tabs"
        x: 0
        y: 0
        width: 480
        height: 44
        visible: detail.inbox.selectedId > 0
        currentIndex: detail.inbox.tab
        // The store's tab and the bar's follow each other; writing only a
        // change keeps the pair from feeding itself.
        onCurrentIndexChanged: {
            if (detail.inbox.tab !== tabs.currentIndex) {
                detail.inbox.tab = tabs.currentIndex
            }
        }

        TabButton {
            width: 160
            text: t`Conversation`
        }
        TabButton {
            width: 160
            text: t`Edit`
        }
        TabButton {
            width: 160
            text: t`Insights`
        }
    }

    Text {
        anchors.centerIn: parent
        visible: detail.inbox.selectedId <= 0
        text: t`Select an issue`
        Accessible.role: Accessible.StaticText
        Accessible.name: text
        color: Theme.textMuted
        font.size: 14
    }

    Panel {
        index: 0
        inbox: detail.inbox
        Conversation {
            anchors.fill: parent
            inbox: detail.inbox
        }
    }

    Panel {
        index: 1
        inbox: detail.inbox
        EditForm {
            anchors.fill: parent
            inbox: detail.inbox
            open: detail.inbox.tab === 1
        }
    }

    Panel {
        index: 2
        inbox: detail.inbox
        Insights {
            anchors.fill: parent
            inbox: detail.inbox
        }
    }
}
