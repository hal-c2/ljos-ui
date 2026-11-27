import Ljos.Ui

// One tab's page under the tabs. The one shown is opaque and in place; the
// others are faded out a little to the side of their tab, so a switch
// slides the new page in from where its tab is.
Item {
    id: panel
    required property int index
    required property Store inbox
    default property alias content: holder.data

    readonly property bool shown: panel.inbox.selectedId > 0 && panel.inbox.tab === panel.index
    readonly property real away: panel.index < panel.inbox.tab ? -24 : 24

    x: 0
    y: 44
    width: parent.width
    height: parent.height - 44
    opacity: panel.shown ? 1 : 0
    visible: panel.opacity > 0
    enabled: panel.shown

    Behavior on opacity {
        NumberAnimation { duration: 180; easing.type: Easing.OutCubic }
    }

    Item {
        id: holder
        x: panel.shown || panel.inbox.reduced ? 0 : panel.away
        y: 0
        width: panel.width
        height: panel.height

        Behavior on x {
            NumberAnimation { duration: panel.inbox.reduced ? 0 : 180; easing.type: Easing.OutCubic }
        }
    }
}
