// The window of `ljos observe` (crates/ljos_cli/src/observe.rs). Every
// value it shows comes from an inspector tool through `client.call`; it
// asks only when a page is opened, an object is picked, a property is
// edited or Refresh is pressed, never on a timer.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Ljos.Observer

ApplicationWindow {
    id: window
    required property string tokenFile

    visible: true
    title: "Observer — " + (client.url === "" ? "connecting" : client.url)
    color: sys.window

    SystemPalette { id: sys }

    // What each answer still awaited is for, by serial: the name of a
    // handler in `answered` below, as a property holds values, not
    // functions.
    property var waiting: ({})

    function ask(tool, args, route) {
        var serial = client.call(tool, JSON.stringify(args || {}))
        var next = waiting
        next[serial] = route
        waiting = next
    }

    function answered(route, ok, r) {
        if (route === "processes" && ok)
            processes.rows = r.structuredContent.processes
        else if (route === "mailboxes" && ok)
            mailboxes.rows = r.structuredContent.mailboxes
        else if (route === "snapshot" && ok)
            objects.showOutline(r.structuredContent.outline)
        else if (route === "inspect")
            objects.showProperties(ok, r)
        else if (route === "picture")
            objects.showPicture(ok, r)
        else if (route === "edited")
            objects.pick(objects.picked)
    }

    // The page shown: 0 processes, 1 mailboxes, 2 objects.
    property int page: 0

    function refresh() {
        if (page === 0)
            processes.load()
        else if (page === 1)
            mailboxes.load()
        else
            objects.load()
    }

    function ms(us) {
        if (us === undefined || us === null)
            return ""
        if (us < 1000)
            return us + " µs"
        if (us < 1000000)
            return (us / 1000).toFixed(1) + " ms"
        return (us / 1000000).toFixed(2) + " s"
    }

    readonly property color accent: sys.highlight
    readonly property color subtle: Qt.rgba(sys.windowText.r, sys.windowText.g, sys.windowText.b, 0.6)
    readonly property color rule: Qt.rgba(sys.windowText.r, sys.windowText.g, sys.windowText.b, 0.12)
    readonly property color busy: "#e0a030"
    readonly property color idle: "#3fae6a"
    readonly property color blocked: "#d9534f"

    InspectorClient {
        id: client
        tokenFile: window.tokenFile
        onAnswered: function(serial, tool, ok, result) {
            var next = window.waiting
            var route = next[serial]
            delete next[serial]
            window.waiting = next
            status.text = ok ? "" : tool + ": " + result
            window.answered(route, ok, ok ? JSON.parse(result) : result)
        }
    }

    Shortcut {
        sequence: "Ctrl+R"
        onActivated: window.refresh()
    }

    Component.onCompleted: refresh()

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // The pages, and whom the window observes.
        Rectangle {
            Layout.fillHeight: true
            Layout.preferredWidth: 200
            color: Qt.darker(sys.window, 1.06)

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 12
                spacing: 4

                Label {
                    text: "Observer"
                    font.pixelSize: 20
                    font.bold: true
                    Layout.bottomMargin: 12
                }

                Repeater {
                    model: ["Processes", "Mailboxes", "Objects"]
                    delegate: ItemDelegate {
                        required property string modelData
                        required property int index
                        Layout.fillWidth: true
                        text: modelData
                        highlighted: window.page === index
                        onClicked: {
                            window.page = index
                            window.refresh()
                        }
                    }
                }

                Item { Layout.fillHeight: true }

                RowLayout {
                    spacing: 6
                    Rectangle {
                        width: 10; height: 10; radius: 5
                        color: client.connected ? window.idle : window.blocked
                    }
                    Label {
                        text: client.pending > 0 ? "asking…"
                            : client.connected ? "connected" : "not connected"
                        color: window.subtle
                    }
                }
                Label {
                    Layout.fillWidth: true
                    text: client.url
                    elide: Text.ElideMiddle
                    color: window.subtle
                    font.pixelSize: 11
                }
                Button {
                    Layout.fillWidth: true
                    text: "Refresh"
                    onClicked: window.refresh()
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            StackLayout {
                id: pages
                currentIndex: window.page
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.margins: 16

                // ----- Processes ---------------------------------------
                ColumnLayout {
                    id: processes
                    spacing: 8
                    property var rows: []
                    property string kind: ""

                    function load() {
                        var args = kind === "" ? {} : { kind: kind }
                        window.ask("app_processes", args, "processes")
                    }

                    RowLayout {
                        Label { text: "Processes"; font.pixelSize: 18; font.bold: true }
                        Item { Layout.fillWidth: true }
                        Repeater {
                            model: [["All", ""], ["Threads", "thread"], ["Timers", "timer"],
                                    ["Animations", "animation"], ["Owners", "owner"]]
                            delegate: Button {
                                required property var modelData
                                text: modelData[0]
                                flat: processes.kind !== modelData[1]
                                onClicked: {
                                    processes.kind = modelData[1]
                                    processes.load()
                                }
                            }
                        }
                    }

                    // Column heads.
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.leftMargin: 8
                        Label { text: "Kind"; Layout.preferredWidth: 90; color: window.subtle }
                        Label { text: "Name"; Layout.fillWidth: true; color: window.subtle }
                        Label { text: "State"; Layout.preferredWidth: 100; color: window.subtle }
                        Label { text: "Work"; Layout.preferredWidth: 90; color: window.subtle; horizontalAlignment: Text.AlignRight }
                        Label { text: "Wakeups"; Layout.preferredWidth: 80; color: window.subtle; horizontalAlignment: Text.AlignRight }
                        Label { text: "Detail"; Layout.preferredWidth: 200; color: window.subtle }
                    }
                    Rectangle { Layout.fillWidth: true; height: 1; color: window.rule }

                    ListView {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        model: processes.rows
                        ScrollBar.vertical: ScrollBar {}
                        delegate: Rectangle {
                            required property var modelData
                            required property int index
                            width: ListView.view.width
                            height: 30
                            color: index % 2 ? sys.alternateBase : "transparent"
                            RowLayout {
                                anchors.fill: parent
                                anchors.leftMargin: 8
                                Label { text: modelData.kind; Layout.preferredWidth: 90; color: window.subtle }
                                Label {
                                    text: modelData.name
                                    Layout.fillWidth: true
                                    elide: Text.ElideRight
                                    font.bold: modelData.name === "UI thread"
                                }
                                RowLayout {
                                    Layout.preferredWidth: 100
                                    Layout.maximumWidth: 100
                                    Layout.minimumWidth: 100
                                    spacing: 6
                                    Rectangle {
                                        width: 8; height: 8; radius: 4
                                        visible: modelData.state !== undefined
                                        color: modelData.state === "busy" || modelData.state === "running" ? window.busy
                                             : modelData.state === "blocked" ? window.blocked : window.idle
                                    }
                                    Label { text: modelData.state || "" }
                                }
                                Label {
                                    text: window.ms(modelData.work_us)
                                    Layout.preferredWidth: 90
                                    horizontalAlignment: Text.AlignRight
                                    font.family: "monospace"
                                }
                                Label {
                                    text: modelData.wakeups === undefined ? "" : modelData.wakeups
                                    Layout.preferredWidth: 80
                                    horizontalAlignment: Text.AlignRight
                                    font.family: "monospace"
                                }
                                Label {
                                    Layout.preferredWidth: 200
                                    elide: Text.ElideRight
                                    color: window.subtle
                                    text: modelData.kind === "timer"
                                          ? "every " + modelData.interval_ms + " ms" + (modelData.repeat ? ", repeats" : "")
                                          : modelData.role || modelData.locator || ""
                                }
                            }
                        }
                    }
                }

                // ----- Mailboxes ---------------------------------------
                ColumnLayout {
                    id: mailboxes
                    spacing: 8
                    property var rows: []

                    function load() {
                        window.ask("app_mailboxes", { peek: 5 }, "mailboxes")
                    }

                    Label { text: "Mailboxes"; font.pixelSize: 18; font.bold: true }
                    Label {
                        text: "Counted since the inspector started. Kinds only, never contents."
                        color: window.subtle
                    }

                    GridView {
                        id: boxes
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        cellWidth: Math.max(280, width / Math.max(1, Math.floor(width / 320)))
                        cellHeight: 160
                        model: mailboxes.rows
                        delegate: Item {
                            required property var modelData
                            width: boxes.cellWidth
                            height: boxes.cellHeight
                            Rectangle {
                                anchors.fill: parent
                                anchors.margins: 6
                                radius: 8
                                color: sys.base
                                border.color: window.rule
                                ColumnLayout {
                                    anchors.fill: parent
                                    anchors.margins: 12
                                    spacing: 6
                                    RowLayout {
                                        Label { text: modelData.name; font.bold: true; font.pixelSize: 15 }
                                        Item { Layout.fillWidth: true }
                                        Label {
                                            text: modelData.depth + " / " + modelData.capacity
                                            font.family: "monospace"
                                        }
                                    }
                                    // How full it is.
                                    Rectangle {
                                        Layout.fillWidth: true
                                        height: 6
                                        radius: 3
                                        color: window.rule
                                        Rectangle {
                                            height: parent.height
                                            radius: 3
                                            width: parent.width * Math.min(1, modelData.depth / Math.max(1, modelData.capacity))
                                            color: modelData.depth >= modelData.capacity ? window.blocked
                                                 : modelData.depth > 0 ? window.busy : window.idle
                                        }
                                    }
                                    GridLayout {
                                        columns: 4
                                        columnSpacing: 12
                                        rowSpacing: 2
                                        Label { text: "sent"; color: window.subtle }
                                        Label { text: modelData.sent; font.family: "monospace" }
                                        Label { text: "drained"; color: window.subtle }
                                        Label { text: modelData.drained; font.family: "monospace" }
                                        Label { text: "refused"; color: window.subtle }
                                        Label {
                                            text: modelData.refused
                                            font.family: "monospace"
                                            color: modelData.refused > 0 ? window.blocked : sys.text
                                        }
                                        Label { text: "replaced"; color: window.subtle }
                                        Label { text: modelData.replaced; font.family: "monospace" }
                                        Label { text: "senders"; color: window.subtle }
                                        Label { text: modelData.senders + (modelData.waiting_senders > 0 ? " (" + modelData.waiting_senders + " waiting)" : ""); font.family: "monospace" }
                                        Label { text: "oldest"; color: window.subtle }
                                        Label { text: modelData.oldest_us === undefined || modelData.oldest_us === null ? "—" : window.ms(modelData.oldest_us); font.family: "monospace" }
                                    }
                                    Label {
                                        Layout.fillWidth: true
                                        elide: Text.ElideRight
                                        color: window.subtle
                                        text: modelData.kinds.length === 0 ? "empty"
                                              : modelData.kinds.map(function(k) { return k.kind + " ×" + k.count }).join(", ")
                                    }
                                    Item { Layout.fillHeight: true }
                                }
                            }
                        }
                    }
                }

                // ----- Objects -----------------------------------------
                RowLayout {
                    id: objects
                    spacing: 12
                    property var outline: []
                    property string picked: ""
                    property var props: []
                    property string head: ""
                    property string picture: ""
                    property string pictureNote: ""

                    function load() {
                        window.ask("app_snapshot", {}, "snapshot")
                    }

                    function showOutline(outline) {
                        var rows = []
                        var lines = outline.split("\n")
                        for (var i = 0; i < lines.length; ++i) {
                            var m = /^(\s*)- (.*?)\s*\[(e\d+)\](.*)$/.exec(lines[i])
                            if (m)
                                rows.push({ depth: m[1].length / 2, label: m[2], ref: m[3], rest: m[4].trim() })
                        }
                        objects.outline = rows
                        if (objects.picked !== "")
                            objects.pick(objects.picked)
                    }

                    function pick(ref) {
                        picked = ref
                        window.ask("app_inspect", { target: ref }, "inspect")
                        window.ask("app_screenshot", { target: ref, max_size: 640 }, "picture")
                    }

                    function showProperties(ok, r) {
                        if (!ok) {
                            objects.props = []
                            objects.head = r
                            return
                        }
                        var s = r.structuredContent
                        objects.head = (s.type || "") + (s.object_name ? "  \"" + s.object_name + "\"" : "")
                        objects.props = s.properties
                    }

                    function showPicture(ok, r) {
                        objects.picture = ""
                        objects.pictureNote = ok ? "" : r
                        if (!ok)
                            return
                        for (var i = 0; i < r.content.length; ++i)
                            if (r.content[i].type === "image")
                                objects.picture = "data:" + r.content[i].mimeType + ";base64," + r.content[i].data
                    }

                    // The text of a field as the JSON the property takes.
                    function typed(type, text) {
                        if (type === "bool")
                            return text === "true"
                        if (["int", "real", "double", "number"].indexOf(type) >= 0 && text.trim() !== "" && !isNaN(Number(text)))
                            return Number(text)
                        return text
                    }

                    function edit(name, type, text) {
                        window.ask("app_set_property",
                                   { target: picked, property: name, value: typed(type, text) },
                                   "edited")
                    }

                    // The tree.
                    ListView {
                        id: tree
                        Layout.preferredWidth: 240
                        Layout.fillHeight: true
                        clip: true
                        model: objects.outline
                        ScrollBar.vertical: ScrollBar {}
                        delegate: ItemDelegate {
                            required property var modelData
                            width: ListView.view.width
                            height: 28
                            highlighted: objects.picked === modelData.ref
                            leftPadding: 8 + modelData.depth * 16
                            text: modelData.label + (modelData.rest === "" ? "" : "  " + modelData.rest)
                            onClicked: objects.pick(modelData.ref)
                        }
                    }

                    // The properties of the one picked.
                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        spacing: 6
                        Label {
                            Layout.fillWidth: true
                            Layout.leftMargin: 12
                            text: objects.picked === "" ? "Pick an object" : objects.picked + "  " + objects.head
                            font.bold: true
                            elide: Text.ElideRight
                        }
                        ListView {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true
                            model: objects.props
                            ScrollBar.vertical: ScrollBar {}
                            delegate: RowLayout {
                                required property var modelData
                                width: ListView.view.width
                                height: 34
                                spacing: 8
                                Label {
                                    text: modelData.name
                                    Layout.preferredWidth: 120
                                    Layout.leftMargin: 12
                                    elide: Text.ElideRight
                                }
                                TextField {
                                    Layout.fillWidth: true
                                    clip: true
                                    text: modelData.value === null ? "" : modelData.value
                                    readOnly: modelData.read_only === true
                                    onAccepted: objects.edit(modelData.name, modelData.type, text)
                                }
                                Label {
                                    text: modelData.provenance
                                    Layout.preferredWidth: 64
                                    color: modelData.provenance === "bound" ? window.accent : window.subtle
                                }
                            }
                        }
                    }

                    // A picture of it.
                    Rectangle {
                        Layout.preferredWidth: 260
                        Layout.fillHeight: true
                        radius: 8
                        color: sys.base
                        border.color: window.rule
                        // At its own size, made smaller only to fit.
                        Image {
                            anchors.centerIn: parent
                            width: Math.min(implicitWidth, parent.width - 24)
                            height: Math.min(implicitHeight, parent.height - 24)
                            fillMode: Image.PreserveAspectFit
                            source: objects.picture
                        }
                        Label {
                            anchors.centerIn: parent
                            width: parent.width - 24
                            wrapMode: Text.Wrap
                            horizontalAlignment: Text.AlignHCenter
                            visible: objects.picture === ""
                            color: window.subtle
                            text: objects.pictureNote !== "" ? objects.pictureNote : "No picture"
                        }
                    }
                }
            }

            // What the last refusal or failure said.
            Label {
                id: status
                Layout.fillWidth: true
                Layout.margins: 8
                elide: Text.ElideRight
                color: window.subtle
                visible: text !== ""
            }
        }
    }
}
