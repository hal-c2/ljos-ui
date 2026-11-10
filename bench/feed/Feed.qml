// A mail-like feed, the same UI as crates/ljos_egui/examples/feed_egui.rs
// draws with egui alone: a toolbar with buttons, a search field and a
// spinner, a sidebar of folders, and a list of a thousand rich rows (a check
// box, an avatar with initials, three lines of text, a tag and a star). The
// spinner runs while everything else is still, which is where drawing only
// what changed pays: one small item moves, nothing else needs drawing.
import QtQuick
import QtQuick.Controls

Rectangle {
    id: root
    width: 1024
    height: 720
    color: "white"

    // Whether the spinner turns; `feed-still.qml` is the feed without it.
    property bool spinning: true
    property int folder: 0

    readonly property var names: ["Ada Lovelace", "Alan Turing", "Grace Hopper", "Edsger Dijkstra",
        "Barbara Liskov", "Donald Knuth", "Margaret Hamilton", "Ken Thompson",
        "Frances Allen", "John Backus", "Radia Perlman", "Niklaus Wirth"]
    readonly property var subjects: ["Quarterly report", "Lunch on Friday?", "Re: the build is red",
        "Design review notes", "Your invoice", "Trip photos", "Meeting moved to 3 pm",
        "Draft for comments", "Welcome aboard", "Release checklist"]
    readonly property var words: ["the", "quick", "brown", "fox", "jumps", "over", "a", "lazy",
        "dog", "while", "light", "falls", "on", "northern", "hills", "and", "every", "small",
        "window", "glows"]
    readonly property var tags: ["work", "family", "travel", "finance", "later"]
    readonly property var tagColors: ["#dbeafe", "#dcfce7", "#fef3c7", "#fce7f3", "#e5e7eb"]
    readonly property var avatarColors: ["#ef4444", "#f97316", "#eab308", "#22c55e",
        "#14b8a6", "#3b82f6", "#8b5cf6", "#ec4899"]
    readonly property var folders: ["Inbox", "Starred", "Sent", "Drafts", "Archive", "Spam",
        "Trash", "Work", "Family", "Travel", "Finance", "Later"]

    function preview(i) {
        let s = []
        for (let k = 0; k < 14; ++k)
            s.push(words[(i * 7 + k * 3) % words.length])
        return s.join(" ")
    }
    function initials(name) {
        const parts = name.split(" ")
        return parts[0][0] + parts[parts.length - 1][0]
    }

    Rectangle {
        id: toolbar
        width: parent.width
        height: 48
        color: "#f3f4f6"
        Text {
            id: title
            x: 12
            anchors.verticalCenter: parent.verticalCenter
            text: root.folders[root.folder]
            font.bold: true
            font.pixelSize: 16
        }
        Row {
            x: 120
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Button { text: "New" }
            Button { text: "Reply" }
            Button { text: "Archive" }
        }
        TextField {
            id: search
            x: 600
            width: 260
            height: 32
            anchors.verticalCenter: parent.verticalCenter
            placeholderText: "Search mail"
        }
        Item {
            id: spinner
            x: parent.width - 36
            width: 20
            height: 20
            anchors.verticalCenter: parent.verticalCenter
            Rectangle {
                anchors.fill: parent
                radius: 10
                color: "transparent"
                border.width: 3
                border.color: "#d1d5db"
            }
            Rectangle {
                x: 7
                width: 6
                height: 6
                radius: 3
                color: "#3b82f6"
            }
            RotationAnimation on rotation {
                from: 0
                to: 360
                duration: 1000
                loops: Animation.Infinite
                running: root.spinning
            }
        }
    }

    Rectangle {
        id: sidebar
        y: toolbar.height
        width: 200
        height: parent.height - toolbar.height
        color: "#f9fafb"
        Column {
            y: 8
            width: parent.width
            Repeater {
                model: root.folders.length
                delegate: AbstractButton {
                    id: entry
                    required property int index
                    width: 200
                    height: 32
                    onClicked: root.folder = index
                    background: Rectangle {
                        color: entry.index === root.folder ? "#dbeafe" : "transparent"
                    }
                    contentItem: Item {
                        Text {
                            x: 16
                            anchors.verticalCenter: parent.verticalCenter
                            text: root.folders[entry.index]
                        }
                        Text {
                            anchors.right: parent.right
                            anchors.rightMargin: 16
                            anchors.verticalCenter: parent.verticalCenter
                            text: (entry.index * 37 + 5) % 120
                            color: "#6b7280"
                        }
                    }
                }
            }
        }
    }

    ListView {
        id: list
        objectName: "list"
        x: sidebar.width
        y: toolbar.height
        width: parent.width - sidebar.width
        height: parent.height - toolbar.height
        clip: true
        model: 1000
        delegate: Rectangle {
            id: row
            required property int index
            readonly property string name: root.names[index % root.names.length]
            width: ListView.view.width
            height: 64
            color: index % 2 ? "#fafafa" : "white"

            CheckBox {
                id: check
                x: 4
                anchors.verticalCenter: parent.verticalCenter
                checked: row.index % 3 === 0
            }
            Rectangle {
                id: avatar
                anchors.left: check.right
                anchors.leftMargin: 4
                anchors.verticalCenter: parent.verticalCenter
                width: 40
                height: 40
                radius: 20
                color: root.avatarColors[row.index % root.avatarColors.length]
                Text {
                    anchors.centerIn: parent
                    text: root.initials(row.name)
                    color: "white"
                    font.bold: true
                }
            }
            Text {
                id: sender
                anchors.left: avatar.right
                anchors.leftMargin: 12
                y: 6
                text: row.name
                font.bold: true
            }
            Text {
                anchors.right: star.left
                anchors.rightMargin: 8
                y: 6
                text: (row.index % 12 + 1) + ":" + String(row.index * 7 % 60).padStart(2, "0")
                color: "#6b7280"
            }
            Text {
                anchors.left: sender.left
                anchors.right: chip.left
                anchors.rightMargin: 8
                y: 24
                text: root.subjects[row.index % root.subjects.length]
                elide: Text.ElideRight
            }
            Text {
                anchors.left: sender.left
                anchors.right: chip.left
                anchors.rightMargin: 8
                y: 42
                text: root.preview(row.index)
                elide: Text.ElideRight
                color: "#6b7280"
                font.pixelSize: 11
            }
            Rectangle {
                id: chip
                anchors.right: star.left
                anchors.rightMargin: 8
                y: 30
                width: tag.implicitWidth + 16
                height: 20
                radius: 10
                color: root.tagColors[row.index % root.tags.length]
                Text {
                    id: tag
                    anchors.centerIn: parent
                    text: root.tags[row.index % root.tags.length]
                    font.pixelSize: 11
                }
            }
            AbstractButton {
                id: star
                property bool on: row.index % 4 === 1
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                width: 32
                height: 32
                onClicked: on = !on
                contentItem: Text {
                    text: star.on ? "★" : "☆"
                    color: star.on ? "#f59e0b" : "#9ca3af"
                    font.pixelSize: 20
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
            }
        }
    }
}
