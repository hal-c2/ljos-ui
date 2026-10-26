// A workspace window as desktop apps lay one out: a sectioned sidebar of
// conversations (buttons with elided text, an unread dot and a timer per
// row), a transcript of wrapped, styled messages, a multi-line composer, and
// a side panel that slides open from nothing, clipped, with tabs and pages
// loaded on first show. What is measured is what using it costs: typing,
// scrolling, hovering rows and opening the panel, each a frame at a time,
// and what a stream of updates costs (the "Stream" button): the last
// message growing a word at a time, a new message now and then, and the
// sidebar rows they belong to changing, as a backend's messages would.
import QtQuick
import QtQuick.Controls

Rectangle {
    id: root
    width: 1200
    height: 800
    color: "#fafafa"

    property bool panelOpen: false
    property int current: 0
    readonly property var words: ["the", "quick", "brown", "fox", "jumps", "over", "a",
        "lazy", "dog", "while", "light", "falls", "on", "northern", "hills", "and",
        "every", "small", "window", "glows", "well-known", "re-read", "tomorrow"]
    readonly property var tabNames: ["Files", "Notes", "Activity"]

    // Opens and closes the panel every half second with no pointer, so
    // that a profiler on a virtual clock (frame_profile --start
    // 'root.cycling = true') sees what the pointer's toggle costs.
    property bool cycling: false
    Timer {
        interval: 500
        repeat: true
        running: root.cycling
        onTriggered: root.panelOpen = !root.panelOpen
    }

    property int tick: 0
    Timer {
        id: stream
        interval: 50
        repeat: true
        onTriggered: {
            root.tick++
            const last = messages.count - 1
            messages.setProperty(last, "body", messages.get(last).body + " " + root.words[root.tick % root.words.length])
            const row = root.tick % 40
            conversations.setProperty(row, "preview", root.sentence(root.tick, 8))
            conversations.setProperty(row, "unread", !conversations.get(row).unread)
            if (root.tick % 20 === 0) {
                messages.append({ sender: "Sender " + (root.tick % 17), body: root.sentence(root.tick, 4) })
                transcript.positionViewAtEnd()
            }
        }
    }

    function sentence(seed, count) {
        let s = []
        for (let i = 0; i < count; ++i)
            s.push(words[(seed * 7 + i * 13) % words.length])
        return s.join(" ")
    }

    ListModel { id: conversations }
    ListModel { id: messages }

    Component.onCompleted: {
        for (let i = 0; i < 300; ++i)
            conversations.append({ title: "Conversation " + i + " " + sentence(i, 3),
                                   preview: sentence(i + 1, 8), group: "Group " + Math.floor(i / 12),
                                   unread: i % 5 === 0 })
        for (let j = 0; j < 2000; ++j)
            messages.append({ sender: "Sender " + (j % 17),
                              body: sentence(j, 6) + " <b>" + sentence(j + 3, 2) + "</b> "
                                    + sentence(j + 5, 10 + j % 30) + " <i>" + sentence(j, 3) + "</i>" })
    }

    ListView {
        id: sidebar
        objectName: "sidebar"
        width: 280
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        clip: true
        reuseItems: true
        model: conversations
        section.property: "group"
        section.delegate: Text {
            required property string section
            width: ListView.view.width
            height: 24
            leftPadding: 8
            verticalAlignment: Text.AlignVCenter
            text: section
            color: "#888"
            font.pixelSize: 11
            font.bold: true
        }
        delegate: AbstractButton {
            id: row
            required property int index
            required property string title
            required property string preview
            required property bool unread
            property int minutes: index % 59
            width: ListView.view.width
            height: 44
            hoverEnabled: true
            onClicked: root.current = index
            background: Rectangle {
                color: row.index === root.current ? "#d6e4fc" : row.hovered ? "#eef3fc" : "transparent"
            }
            contentItem: Item {
                Rectangle {
                    id: dot
                    width: 8
                    height: 8
                    radius: 4
                    color: row.unread ? "#3478f6" : "transparent"
                    anchors.left: parent.left
                    anchors.leftMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                }
                Text {
                    id: title
                    anchors.left: dot.right
                    anchors.leftMargin: 8
                    anchors.right: age.left
                    anchors.rightMargin: 8
                    anchors.top: parent.top
                    anchors.topMargin: 5
                    text: row.title
                    elide: Text.ElideRight
                    font.bold: row.unread
                }
                Text {
                    anchors.left: title.left
                    anchors.right: parent.right
                    anchors.rightMargin: 8
                    anchors.top: title.bottom
                    text: row.preview
                    elide: Text.ElideRight
                    color: "#666"
                    font.pixelSize: 11
                }
                Text {
                    id: age
                    anchors.right: parent.right
                    anchors.rightMargin: 8
                    anchors.top: parent.top
                    anchors.topMargin: 5
                    text: row.minutes + " min"
                    color: "#999"
                    font.pixelSize: 11
                }
            }
            Timer {
                interval: 60000
                repeat: true
                running: true
                onTriggered: row.minutes++
            }
        }
    }

    Rectangle {
        id: bar
        anchors.left: sidebar.right
        anchors.right: panel.left
        height: 40
        color: "#f0f0f0"
        Text {
            anchors.left: parent.left
            anchors.leftMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            text: "Conversation " + root.current
            font.bold: true
        }
        AbstractButton {
            objectName: "stream"
            anchors.right: toggle.left
            width: 80
            height: parent.height
            onClicked: stream.running = !stream.running
            contentItem: Text {
                text: stream.running ? "Pause" : "Stream"
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
            }
        }
        AbstractButton {
            id: toggle
            objectName: "toggle"
            anchors.right: parent.right
            width: 80
            height: parent.height
            onClicked: root.panelOpen = !root.panelOpen
            contentItem: Text {
                text: root.panelOpen ? "Close" : "Details"
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
            }
        }
    }

    ListView {
        id: transcript
        objectName: "transcript"
        anchors.left: sidebar.right
        anchors.right: panel.left
        anchors.top: bar.bottom
        anchors.bottom: composer.top
        clip: true
        spacing: 8
        model: messages
        delegate: Column {
            required property string sender
            required property string body
            width: ListView.view.width
            leftPadding: 12
            rightPadding: 12
            Text {
                text: parent.sender
                font.bold: true
            }
            Text {
                width: parent.width - 24
                text: parent.body
                textFormat: Text.StyledText
                wrapMode: Text.Wrap
            }
        }
    }

    TextArea {
        id: composer
        objectName: "composer"
        anchors.left: sidebar.right
        anchors.right: panel.left
        anchors.bottom: parent.bottom
        height: Math.min(Math.max(48, implicitHeight), 140)
        wrapMode: TextEdit.Wrap
        placeholderText: "Write a message"
        background: Rectangle {
            color: "white"
            border.color: composer.activeFocus ? "#3478f6" : "#ccc"
        }
    }

    Rectangle {
        id: panel
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: root.panelOpen ? 360 : 0
        Behavior on width {
            NumberAnimation { duration: 250 }
        }
        clip: true
        color: "#f4f4f4"

        ListView {
            id: tabs
            x: 0
            width: 360
            height: 32
            orientation: ListView.Horizontal
            model: root.panelOpen ? root.tabNames : []
            delegate: AbstractButton {
                required property int index
                required property string modelData
                width: 120
                height: 32
                onClicked: tabs.currentIndex = index
                contentItem: Text {
                    text: modelData
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    font.bold: tabs.currentIndex === index
                }
            }
        }

        Repeater {
            model: 3
            Loader {
                required property int index
                property bool shown: tabs.currentIndex === index
                y: 32
                width: 360
                height: panel.height - 32
                visible: shown
                active: false
                onShownChanged: if (shown) active = true
                Component.onCompleted: {
                    setSource(Qt.resolvedUrl("workspace/Page.qml"), { "seed": index })
                    active = shown
                }
            }
        }
    }
}
