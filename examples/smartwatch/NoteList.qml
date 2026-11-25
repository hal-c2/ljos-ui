import Ljos.Ui
import Ljos.Layout
import Ljos.Core
import Ljos.Design
import "logic/notes.ts" as Notes

// What came in, newest first. The crown moves the highlight and pressing
// it (Return) opens the note; a tap opens one too. Clear all empties it.
WatchScreen {
    id: list

    readonly property real rowHeight: 76
    readonly property real listTop: list.round ? 92 : 64
    // The highlighted row stays in sight as the crown moves it.
    readonly property real scroll: Math.max(0, list.watch.choice - 1) * (list.rowHeight + 8)
    property real shownScroll: list.scroll
    Behavior on shownScroll {
        NumberAnimation {
            duration: Motion.moveSmall
            easing.type: Easing.OutCubic
        }
    }

    Title {
        theme: list.theme
        y: list.round ? 48 : 22
        text: list.watch.unread > 0 ? t`${list.watch.unread} NEW` : t`NOTIFICATIONS`
    }

    Item {
        id: viewport
        x: list.inset - 8
        y: list.listTop
        width: list.width - 2 * (list.inset - 8)
        height: list.height - list.listTop - (list.round ? 110 : 96)
        clip: true

        Column {
            y: -list.shownScroll
            width: viewport.width
            spacing: 8
            Repeater {
                model: list.watch.inbox
                delegate: Item {
                    id: row
                    required property Notes.Note modelData
                    required property int index
                    readonly property bool chosen: row.index === list.watch.choice
                    width: viewport.width
                    height: list.rowHeight

                    Accessible.role: Accessible.Button
                    Accessible.name: t`Open the message from ${row.modelData.from}`
                    Accessible.description: list.watch.noteTitle(row.modelData.kind)
                    Accessible.onPressAction: list.watch.open(row.modelData.id)

                    scale: tap.pressed ? Motion.pressScale : 1
                    PressBehavior on scale {}

                    Rectangle {
                        anchors.fill: parent
                        radius: 22
                        fill: row.chosen ? list.theme.selected : list.theme.surface
                        border.color: row.chosen ? list.theme.cyan : list.theme.line
                        border.width: row.chosen ? 2 : 1
                    }
                    Icon {
                        x: 16
                        anchors.verticalCenter: parent.verticalCenter
                        width: 26
                        height: 26
                        glyph: row.modelData.glyph
                        color: row.modelData.kind === "battery" ? list.theme.amber : list.theme.cyan
                    }
                    Text {
                        id: from
                        x: 56
                        y: 12
                        text: row.modelData.from
                        color: list.theme.text
                        font.size: 17
                        font.weight: Font.DemiBold
                    }
                    Text {
                        anchors.right: parent.right
                        anchors.rightMargin: 18
                        anchors.baseline: from.baseline
                        text: list.watch.ago(row.modelData.minutesAgo)
                        color: list.theme.muted
                        font.size: 14
                    }
                    Text {
                        x: 56
                        anchors.top: from.bottom
                        anchors.topMargin: 4
                        width: row.width - 56 - 18
                        text: list.watch.noteTitle(row.modelData.kind)
                        color: list.theme.muted
                        font.size: 15
                        elide: Text.ElideRight
                    }
                    Tap {
                        id: tap
                        onTapped: list.watch.open(row.modelData.id)
                    }
                }
            }
        }
    }

    Column {
        anchors.centerIn: parent
        spacing: 10
        visible: list.watch.unread === 0
        Icon {
            Layout.alignment: Align.HCenter
            width: 40
            height: 40
            glyph: "check"
            color: list.theme.green
        }
        Text {
            Layout.alignment: Align.HCenter
            text: t`All caught up`
            color: list.theme.text
            font.size: 20
        }
    }

    Chip {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: list.round ? 40 : 28
        visible: list.watch.unread > 0
        theme: list.theme
        glyph: "close"
        label: t`Clear all`
        onPressed: list.watch.clearAll()
    }
}
