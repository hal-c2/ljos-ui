import Ljos.Ui
import Ljos.Core
import Ljos.Controls

// The filters: four views and one row per label, one active at a time,
// and the settings at the foot.
Rectangle {
    id: side
    required property Store inbox
    signal openSettings()
    fill: Theme.surface

    NavRow {
        x: 8
        y: 16
        width: 204
        height: 40
        iconName: "inbox"
        label: t`Inbox`
        active: side.inbox.filter === "inbox"
        onPicked: side.inbox.filter = "inbox"
    }

    NavRow {
        x: 8
        y: 56
        width: 204
        height: 40
        iconName: "pin"
        label: t`Assigned to me`
        active: side.inbox.filter === "assigned"
        onPicked: side.inbox.filter = "assigned"
    }

    NavRow {
        x: 8
        y: 96
        width: 204
        height: 40
        iconName: "star"
        label: t`Starred`
        active: side.inbox.filter === "starred"
        onPicked: side.inbox.filter = "starred"
    }

    NavRow {
        x: 8
        y: 136
        width: 204
        height: 40
        iconName: "circle-check"
        label: t`Closed`
        active: side.inbox.filter === "closed"
        onPicked: side.inbox.filter = "closed"
    }

    Text {
        elide: Text.ElideRight
        x: 20
        y: 184
        width: 180
        height: 24
        verticalAlignment: Text.AlignVCenter
        text: t`Labels`
        color: Theme.textMuted
        font.size: 12
        font.weight: Font.DemiBold
    }

    ListView {
        id: labels
        x: 8
        y: 210
        width: 204
        height: 476
        clip: true
        model: side.inbox.tags
        Accessible.role: Accessible.List
        Accessible.name: t`Labels`
        delegate: NavRow {
            required property string modelData
            width: 204
            height: 28
            iconName: "bookmark"
            label: modelData
            active: side.inbox.filter === "tag:" + modelData
            onPicked: side.inbox.filter = "tag:" + modelData
        }
    }

    Button {
        objectName: "settings"
        x: 8
        y: 692
        width: 204
        height: 32
        subtle: true
        iconName: "settings"
        text: t`Settings`
        onClicked: side.openSettings()
    }
}
