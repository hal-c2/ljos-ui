import Ljos.Ui
import Ljos.Layout
import Ljos.Core

// One notification, read in full, with what can be done about it.
WatchScreen {
    id: detail

    readonly property real wide: detail.width - 2 * detail.inset

    Icon {
        id: mark
        anchors.horizontalCenter: parent.horizontalCenter
        y: detail.round ? 48 : 30
        width: 30
        height: 30
        glyph: detail.watch.note.glyph
        color: detail.theme.cyan
    }
    Text {
        id: from
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: mark.bottom
        anchors.topMargin: 6
        text: `${detail.watch.note.from} · ${detail.watch.ago(detail.watch.note.minutesAgo)}`
        color: detail.theme.muted
        font.size: 15
    }
    Text {
        id: heading
        x: detail.inset
        anchors.top: from.bottom
        anchors.topMargin: 12
        width: detail.wide
        text: `${detail.watch.noteTitle(detail.watch.note.kind)}`
        color: detail.theme.text
        font.size: 22
        font.weight: Font.DemiBold
        align: Text.AlignHCenter
        wrap: Text.WordWrap
    }
    Text {
        x: detail.inset
        anchors.top: heading.bottom
        anchors.topMargin: 10
        width: detail.wide
        text: `${detail.watch.noteBody(detail.watch.note.kind)}`
        color: detail.theme.muted
        font.size: 17
        lineHeight: 1.25
        align: Text.AlignHCenter
        wrap: Text.WordWrap
    }

    Row {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: detail.round ? 40 : 28
        spacing: 12
        Chip {
            theme: detail.theme
            glyph: "back"
            help: t`Back`
            onPressed: detail.watch.back()
        }
        Chip {
            theme: detail.theme
            glyph: "check"
            label: t`Dismiss`
            primary: true
            onPressed: detail.watch.dismiss(detail.watch.note.id)
        }
    }
}
