import Ljos.Ui
import Ljos.Core
import Ljos.Controls
import Ljos.Layout
import "logic/inbox.ts" as Inbox

// The record as it reads: title, who and when, its state, its labels and
// its description. Choosing another record fades the new one in.
Item {
    id: page
    required property Store inbox
    readonly property Inbox.Issue issue: page.inbox.selected

    readonly property int shownId: page.inbox.selectedId
    onShownIdChanged: fade.restart()

    NumberAnimation {
        id: fade
        target: content
        property: "opacity"
        from: 0
        to: 1
        duration: 120
        easing.type: Easing.OutCubic
    }

    Button {
        objectName: "close-issue"
        z: 1
        x: 436
        y: 16
        width: 120
        height: 32
        text: page.issue.status === "open" ? t`Close issue` : t`Reopen`
        onClicked: page.inbox.toggleStatus()
    }

    Flickable {
        id: content
        x: 20
        y: 16
        width: page.width - 40
        height: page.height - 32
        clip: true
        contentWidth: width
        contentHeight: column.height
        boundsBehavior: Flickable.StopAtBounds

        Column {
            id: column
            width: content.width
            spacing: 12

            Text {
                width: 400
                text: page.issue.title
                color: Theme.text
                font.size: 20
                font.weight: Font.DemiBold
                wrap: Text.Wrap
            }

            Row {
                spacing: 12

                Avatar {
                    size: 48
                    source: page.issue.avatar
                }

                Column {
                    spacing: 4

                    Text {
                        text: page.issue.author
                        color: Theme.text
                        font.size: 14
                        font.weight: Font.Medium
                    }

                    Text {
                        text: `${page.inbox.day(page.issue.created)} · ${page.inbox.ago(page.issue.created)}`
                        color: Theme.textMuted
                        font.size: 12
                    }
                }
            }

            Row {
                spacing: 8

                Rectangle {
                    width: status.implicitWidth + 16
                    height: 22
                    radius: 11
                    fill: page.issue.status === "open" ? Theme.accent : Theme.surfaceOverlay

                    Text {
                        id: status
                        anchors.centerIn: parent
                        text: page.issue.status === "open" ? t`Open` : t`Closed`
                        color: page.issue.status === "open" ? Theme.accentForeground : Theme.text
                        font.size: 12
                        font.weight: Font.DemiBold
                    }
                }

                Text {
                    height: 22
                    verticalAlignment: Text.AlignVCenter
                    text: page.issue.assignee === "" ? t`Nobody` : page.issue.assignee
                    color: Theme.textMuted
                    font.size: 12
                    Accessible.name: t`Assignee`
                }

                Repeater {
                    model: page.issue.tags
                    delegate: Chip {
                        required property string modelData
                        y: 2
                        text: modelData
                    }
                }
            }

            Repeater {
                model: page.issue.body.split("\n\n")
                delegate: TextEdit {
                    required property string modelData
                    width: Math.min(column.width, 680)
                    text: modelData
                    readOnly: true
                    selectByMouse: true
                    wrapMode: TextEdit.Wrap
                    color: Theme.text
                    selectionColor: Theme.accentSurface
                    font.pixelSize: 14
                }
            }
        }
    }
}
