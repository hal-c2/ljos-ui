// The window of `ljos observe` (crates/ljos_cli/src/observe.rs). Every
// value it shows comes from an inspector tool through `client.call`.
//
// It follows the application with `app_watch`, which the inspector
// answers on its server's thread when a pass is drawn, a line is logged or
// a reload happens, and holds otherwise: while the application rests,
// neither it nor this window does any work. Each answer brings the page
// shown up to date and adds a point to the charts.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Ljos.Observer

ApplicationWindow {
    id: window
    required property string tokenFile

    visible: true
    title: "Observer — " + (client.url === "" ? "connecting" : client.url)
    color: theme.bg

    // The system's scheme, light or dark, as the rest of the desktop.
    readonly property bool dark: Qt.styleHints.colorScheme === Qt.ColorScheme.Dark

    // The controls inherit the window's palette: a field, a box to tick
    // and a button are drawn in the theme's colours, not the style's.
    palette.window: theme.bg
    palette.windowText: theme.text
    palette.base: theme.card
    palette.alternateBase: theme.cardAlt
    palette.text: theme.text
    palette.placeholderText: theme.subtle
    palette.button: window.dark ? "#2a2f38" : "#e2e5ea"
    palette.buttonText: theme.text
    palette.highlight: theme.accent
    palette.highlightedText: window.dark ? "#15171c" : "#ffffff"
    palette.mid: theme.rule
    palette.dark: theme.subtle
    palette.light: theme.cardAlt
    palette.midlight: theme.rule

    QtObject {
        id: theme
        readonly property color bg: window.dark ? "#15171c" : "#f4f5f7"
        readonly property color side: window.dark ? "#1b1e24" : "#e9ebef"
        readonly property color card: window.dark ? "#20242b" : "#ffffff"
        readonly property color cardAlt: window.dark ? "#252a32" : "#f6f7f9"
        readonly property color text: window.dark ? "#e4e7ec" : "#1b1e23"
        readonly property color subtle: window.dark ? "#8f97a4" : "#636a75"
        readonly property color rule: window.dark ? "#2e333c" : "#dfe2e7"
        readonly property color accent: window.dark ? "#78a9ff" : "#2c6bd9"
        readonly property color accentSoft: Qt.rgba(accent.r, accent.g, accent.b, window.dark ? 0.22 : 0.12)
        readonly property color good: window.dark ? "#4cc27c" : "#2f9e5c"
        readonly property color warn: window.dark ? "#f0b44c" : "#c98a12"
        readonly property color bad: window.dark ? "#f07068" : "#d2443c"
        readonly property color violet: window.dark ? "#b896ff" : "#7a4fd6"
        readonly property string mono: "monospace"
    }

    // ----- Asking -----------------------------------------------------

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
        var s = ok ? r.structuredContent : null
        if (route === "watch")
            watcher.answered(ok, s)
        else if (route === "processes" && ok)
            processes.show(s)
        else if (route === "mailboxes" && ok)
            mailboxes.show(s.mailboxes)
        else if (route === "objects")
            objects.showTree(ok, s, r)
        else if (route === "inspect")
            objects.showProperties(ok, s, r)
        else if (route === "picture")
            objects.showPicture(ok, r)
        else if (route === "edited")
            objects.edited(ok, r)
        else if (route === "logs" && ok)
            logs.add(s)
        else if (route === "trace" && ok)
            trace.add(s)
        else if (route.indexOf("sample-") === 0 && ok)
            overview.part(route.substring(7), s)
    }

    InspectorClient {
        id: client
        tokenFile: window.tokenFile
        onAnswered: function(serial, tool, ok, result) {
            var next = window.waiting
            var route = next[serial]
            delete next[serial]
            window.waiting = next
            // A refresh the inspector turned away for being asked too
            // often keeps what is shown; the next change asks again. An
            // edit turned away is told.
            if (!ok && route !== "watch" && route !== "edited"
                    && String(result).indexOf("rate_limited") >= 0) {
                status.text = "The application is asked too often; showing the last answer"
                return
            }
            if (route !== "watch")
                status.text = ok ? "" : tool + ": " + result
            window.answered(route || "", ok, ok ? JSON.parse(result) : result)
        }
    }

    // The page shown.
    readonly property var pages: ["Overview", "Processes", "Mailboxes", "Objects", "Logs", "Trace"]
    property int page: 0

    function refresh(changed) {
        var all = changed === undefined
        overview.sample()
        if (page === 1)
            processes.load()
        else if (page === 2)
            mailboxes.load()
        else if (page === 3 && (all || changed.indexOf("paints") >= 0 || changed.indexOf("reloads") >= 0))
            objects.load(all || changed.indexOf("reloads") >= 0)
        if (all || changed.indexOf("logs") >= 0)
            logs.load()
        if (all || changed.indexOf("traces") >= 0)
            trace.load()
    }

    // Follows the application: one `app_watch` waiting at a time, asked
    // again when it answers. A busy application is shown once a second:
    // each showing costs a handful of calls, and the inspector takes
    // twenty a second from everyone.
    QtObject {
        id: watcher
        property var counts: null
        property bool live: true
        property bool waiting: false
        // When the last answer said nothing changed.
        property string quietSince: ""

        function start() {
            if (waiting || !live)
                return
            waiting = true
            var args = counts === null ? {}
                : { since: { paints: counts.paints, logs: counts.logs, reloads: counts.reloads,
                            traces: counts.traces },
                    wait_ms: 30000, settle_ms: 1000 }
            window.ask("app_watch", args, "watch")
        }

        function answered(ok, s) {
            waiting = false
            if (!ok) {
                // The application is gone or not there yet: try again
                // in a while, and only while that is so.
                retry.start()
                return
            }
            var first = counts === null
            counts = s.counts
            if (first)
                window.refresh()
            else if (s.changed.length > 0) {
                quietSince = ""
                window.refresh(s.changed)
            } else if (quietSince === "")
                quietSince = Qt.formatTime(new Date(), "hh:mm:ss")
            start()
        }

        onLiveChanged: if (live) start()
    }

    Timer {
        id: retry
        interval: 2000
        onTriggered: watcher.start()
    }

    Shortcut {
        sequence: "Ctrl+R"
        onActivated: window.refresh()
    }

    Component.onCompleted: watcher.start()

    function ms(us) {
        if (us === undefined || us === null)
            return ""
        if (us < 1000)
            return us + " µs"
        if (us < 1000000)
            return (us / 1000).toFixed(1) + " ms"
        return (us / 1000000).toFixed(2) + " s"
    }

    function bytes(n) {
        if (n === undefined || n === null)
            return "—"
        if (n < 1024 * 1024)
            return (n / 1024).toFixed(0) + " KiB"
        return (n / (1024 * 1024)).toFixed(1) + " MiB"
    }

    function stateColor(state) {
        if (state === "running" || state === "busy")
            return theme.warn
        if (state === "blocked")
            return theme.bad
        return theme.good
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // ----- The pages, and whom the window observes ------------------
        Rectangle {
            Layout.fillHeight: true
            Layout.preferredWidth: 210
            color: theme.side

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 14
                spacing: 2

                Label {
                    text: "Observer"
                    color: theme.text
                    font.pixelSize: 21
                    font.bold: true
                }
                Label {
                    Layout.bottomMargin: 18
                    text: processes.process ? "pid " + processes.process.pid : "ljos observe"
                    color: theme.subtle
                }

                Repeater {
                    model: window.pages
                    delegate: Rectangle {
                        required property string modelData
                        required property int index
                        Layout.fillWidth: true
                        height: 34
                        radius: 7
                        color: window.page === index ? theme.accentSoft
                             : hover.containsMouse ? Qt.rgba(theme.text.r, theme.text.g, theme.text.b, 0.05)
                             : "transparent"
                        Rectangle {
                            visible: window.page === index
                            width: 3; height: 16; radius: 2
                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            color: theme.accent
                        }
                        Label {
                            anchors.left: parent.left
                            anchors.leftMargin: 14
                            anchors.verticalCenter: parent.verticalCenter
                            text: modelData
                            color: window.page === index ? theme.accent : theme.text
                            font.bold: window.page === index
                        }
                        Label {
                            anchors.right: parent.right
                            anchors.rightMargin: 10
                            anchors.verticalCenter: parent.verticalCenter
                            visible: (index === 4 && logs.unread > 0) || (index === 5 && trace.unread > 0)
                            text: index === 4 ? logs.unread : trace.unread
                            color: theme.subtle
                            font.pixelSize: 11
                        }
                        MouseArea {
                            id: hover
                            anchors.fill: parent
                            hoverEnabled: true
                            onClicked: {
                                window.page = index
                                window.refresh()
                            }
                        }
                    }
                }

                Item { Layout.fillHeight: true }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Rectangle {
                        width: 9; height: 9; radius: 5
                        color: !client.connected ? theme.bad
                             : watcher.live ? theme.good : theme.subtle
                    }
                    Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        text: !client.connected ? "not connected"
                            : !watcher.live ? "paused"
                            : watcher.quietSince !== "" ? "idle since " + watcher.quietSince
                            : "live"
                        color: theme.subtle
                    }
                }
                Label {
                    Layout.fillWidth: true
                    text: client.url
                    elide: Text.ElideMiddle
                    color: theme.subtle
                    font.pixelSize: 11
                }
                RowLayout {
                    Layout.topMargin: 6
                    Button {
                        Layout.fillWidth: true
                        text: watcher.live ? "Pause" : "Follow"
                        onClicked: watcher.live = !watcher.live
                    }
                    Button {
                        Layout.fillWidth: true
                        text: "Refresh"
                        onClicked: window.refresh()
                    }
                }
            }
            Rectangle {
                anchors.right: parent.right
                width: 1; height: parent.height
                color: theme.rule
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            StackLayout {
                currentIndex: window.page
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.margins: 18

                // ----- Overview ----------------------------------------
                ColumnLayout {
                    id: overview
                    spacing: 12

                    // One point per answer of the watch: what moved since
                    // the point before.
                    property var points: []
                    property int version: 0
                    property var last: null
                    property var parts: ({})
                    property var latest: ({})

                    function sample() {
                        parts = {}
                        window.ask("app_metrics", {}, "sample-metrics")
                        window.ask("app_processes", { kind: "thread" }, "sample-threads")
                    }

                    function part(name, s) {
                        var next = parts
                        next[name] = s
                        parts = next
                        // The mailboxes are counted after the other two
                        // are answered, so that the depth is not this
                        // window's own questions waiting.
                        if (name !== "mailboxes" && parts.metrics && parts.threads)
                            window.ask("app_processes", { kind: "mailbox" }, "sample-mailboxes")
                        if (parts.metrics && parts.threads && parts.mailboxes)
                            add(parts.metrics, parts.threads, parts.mailboxes)
                    }

                    function add(m, t, b) {
                        if (t.process)
                            processes.process = t.process
                        var now = Date.now()
                        var wakeups = 0
                        for (var i = 0; i < t.processes.length; ++i)
                            wakeups += t.processes[i].wakeups || 0
                        var depth = 0
                        for (var j = 0; j < b.mailboxes.length; ++j)
                            depth += b.mailboxes[j].depth
                        var p = m.paint || {}
                        var here = {
                            t: now,
                            frames: m.frames.count,
                            frameTotal: m.frames.total_us,
                            drawn: (p.presented || 0) + (p.presented_in_part || 0),
                            skipped: p.unchanged || 0,
                            wakeups: wakeups,
                        }
                        var point = {
                            frameMs: 0, drawn: 0, skipped: 0, wakeups: 0,
                            rss: t.process ? t.process.rss_bytes / (1024 * 1024) : 0,
                            inbox: depth,
                        }
                        if (last !== null) {
                            var secs = Math.max(0.001, (now - last.t) / 1000)
                            var frames = here.frames - last.frames
                            point.frameMs = frames > 0 ? (here.frameTotal - last.frameTotal) / frames / 1000 : 0
                            point.drawn = here.drawn - last.drawn
                            point.skipped = here.skipped - last.skipped
                            point.wakeups = (here.wakeups - last.wakeups) / secs
                        }
                        last = here
                        latest = {
                            frames: m.frames.count,
                            overruns: m.frames.overruns,
                            objects: m.objects,
                            threads: t.process ? t.process.threads : t.processes.length,
                            rss: t.process ? t.process.rss_bytes : null,
                            drawn: here.drawn,
                            skipped: here.skipped,
                            painter: p.painter || "",
                        }
                        var all = points.slice(Math.max(0, points.length - 119))
                        all.push(point)
                        points = all
                        version += 1
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        Label { text: "Overview"; color: theme.text; font.pixelSize: 19; font.bold: true }
                        Item { Layout.fillWidth: true }
                        Label {
                            color: theme.subtle
                            text: "one point each time the application changed"
                        }
                    }

                    // The figures now.
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 10
                        Repeater {
                            model: [
                                ["Frames", "frames"], ["Over budget", "overruns"], ["Objects", "objects"],
                                ["Threads", "threads"], ["Resident", "rss"], ["Passes drawn", "drawn"],
                                ["Passes skipped", "skipped"]
                            ]
                            delegate: Rectangle {
                                required property var modelData
                                Layout.fillWidth: true
                                height: 62
                                radius: 9
                                color: theme.card
                                border.color: theme.rule
                                Column {
                                    anchors.fill: parent
                                    anchors.margins: 10
                                    spacing: 2
                                    Label {
                                        text: modelData[0]
                                        color: theme.subtle
                                        font.pixelSize: 11
                                    }
                                    Label {
                                        property var v: overview.latest[modelData[1]]
                                        text: v === undefined || v === null ? "—"
                                            : modelData[1] === "rss" ? window.bytes(v) : v
                                        color: modelData[1] === "overruns" && v > 0 ? theme.bad : theme.text
                                        font.pixelSize: 19
                                        font.bold: true
                                    }
                                }
                            }
                        }
                    }

                    GridLayout {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        columns: 2
                        columnSpacing: 12
                        rowSpacing: 12

                        Repeater {
                            model: [
                                { title: "Frame time", key: "frameMs", unit: "ms", tone: "accent" },
                                { title: "Passes drawn and skipped", key: "drawn", second: "skipped", unit: "", tone: "good" },
                                { title: "Memory, resident", key: "rss", unit: "MiB", tone: "violet" },
                                { title: "Wakeups per second, all threads", key: "wakeups", unit: "/s", tone: "warn" },
                                { title: "Messages waiting in the mailboxes", key: "inbox", unit: "", tone: "bad" }
                            ]
                            delegate: Rectangle {
                                id: chart
                                required property var modelData
                                required property int index
                                readonly property color tone: theme[modelData.tone]
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                Layout.columnSpan: index === 4 ? 2 : 1
                                Layout.minimumHeight: 120
                                radius: 9
                                color: theme.card
                                border.color: theme.rule

                                function series(key) {
                                    var out = []
                                    for (var i = 0; i < overview.points.length; ++i)
                                        out.push(overview.points[i][key])
                                    return out
                                }
                                readonly property var values: { overview.version; return series(modelData.key) }
                                readonly property var seconds: { overview.version; return modelData.second ? series(modelData.second) : [] }
                                readonly property real now: values.length === 0 ? 0 : values[values.length - 1]

                                Label {
                                    x: 12; y: 10
                                    text: chart.modelData.title
                                    color: theme.subtle
                                }
                                Label {
                                    anchors.right: parent.right
                                    anchors.rightMargin: 12
                                    y: 8
                                    text: chart.values.length === 0 ? "—"
                                        : (Math.round(chart.now * 10) / 10) + (chart.modelData.unit === "" ? "" : " " + chart.modelData.unit)
                                          + (chart.modelData.second && chart.seconds.length > 0
                                             ? " drawn  ·  " + chart.seconds[chart.seconds.length - 1] + " skipped" : "")
                                    color: theme.text
                                    font.bold: true
                                }
                                Canvas {
                                    id: plot
                                    anchors.fill: parent
                                    anchors.topMargin: 34
                                    anchors.margins: 12
                                    property int seen: overview.version
                                    property bool darkNow: window.dark
                                    onSeenChanged: requestPaint()
                                    onDarkNowChanged: requestPaint()
                                    onWidthChanged: requestPaint()
                                    onHeightChanged: requestPaint()
                                    onPaint: {
                                        var ctx = getContext("2d")
                                        ctx.clearRect(0, 0, width, height)
                                        ctx.strokeStyle = theme.rule
                                        ctx.lineWidth = 1
                                        for (var g = 0; g <= 2; ++g) {
                                            var gy = Math.round(height * g / 2) + 0.5
                                            ctx.beginPath()
                                            ctx.moveTo(0, gy)
                                            ctx.lineTo(width, gy)
                                            ctx.stroke()
                                        }
                                        var a = chart.values
                                        var b = chart.seconds
                                        if (a.length === 0) {
                                            ctx.fillStyle = theme.subtle
                                            ctx.font = "12px sans-serif"
                                            ctx.fillText("waiting for the application to change", 4, height / 2 - 6)
                                            return
                                        }
                                        var top = 0
                                        for (var i = 0; i < a.length; ++i)
                                            top = Math.max(top, a[i])
                                        for (var k = 0; k < b.length; ++k)
                                            top = Math.max(top, b[k])
                                        top = top <= 0 ? 1 : top * 1.15
                                        var step = width / 119
                                        function line(vs, colour, fill) {
                                            var x0 = width - (vs.length - 1) * step
                                            ctx.beginPath()
                                            for (var n = 0; n < vs.length; ++n) {
                                                var x = x0 + n * step
                                                var y = height - vs[n] / top * height
                                                if (n === 0)
                                                    ctx.moveTo(x, y)
                                                else
                                                    ctx.lineTo(x, y)
                                            }
                                            ctx.strokeStyle = colour
                                            ctx.lineWidth = 2
                                            ctx.stroke()
                                            if (fill) {
                                                ctx.lineTo(x0 + (vs.length - 1) * step, height)
                                                ctx.lineTo(x0, height)
                                                ctx.closePath()
                                                ctx.fillStyle = Qt.rgba(colour.r, colour.g, colour.b, 0.14)
                                                ctx.fill()
                                            }
                                            if (vs.length === 1) {
                                                ctx.fillStyle = colour
                                                ctx.fillRect(x0 - 2, height - vs[0] / top * height - 2, 4, 4)
                                            }
                                        }
                                        if (b.length > 0)
                                            line(b, theme.accent, false)
                                        line(a, chart.tone, true)
                                        ctx.fillStyle = theme.subtle
                                        ctx.font = "10px sans-serif"
                                        ctx.fillText(String(Math.round(top * 10) / 10), 2, 10)
                                    }
                                }
                            }
                        }
                    }
                }

                // ----- Processes ---------------------------------------
                ColumnLayout {
                    id: processes
                    spacing: 10
                    property var rows: []
                    // The view is given the number of rows, not the rows:
                    // a view given a new array makes every row again, and
                    // a busy application is shown anew each second.
                    property int count: 0
                    property var process: null
                    property string kind: ""
                    // Kind, name, state, work, wakeups, detail: the heads
                    // and the rows share them, so they line up.
                    readonly property var widths: [118, 0, 110, 96, 86, 230]

                    function load() {
                        var args = kind === "" ? {} : { kind: kind }
                        window.ask("app_processes", args, "processes")
                    }

                    function show(s) {
                        rows = s.processes
                        count = rows.length
                        if (s.process)
                            process = s.process
                    }

                    function nameWidth(total) {
                        var rest = 0
                        for (var i = 0; i < widths.length; ++i)
                            rest += widths[i]
                        return Math.max(120, total - rest - 16)
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        Label { text: "Processes"; color: theme.text; font.pixelSize: 19; font.bold: true }
                        Label {
                            Layout.leftMargin: 12
                            visible: processes.process !== null
                            color: theme.subtle
                            text: processes.process === null ? ""
                                : processes.process.threads + " threads · " + window.bytes(processes.process.rss_bytes)
                                  + " resident · " + window.ms(processes.process.cpu_us) + " CPU"
                        }
                        Item { Layout.fillWidth: true }
                        Repeater {
                            model: [["All", ""], ["Threads", "thread"], ["Timers", "timer"],
                                    ["Animations", "animation"], ["Owners", "owner"]]
                            delegate: Button {
                                required property var modelData
                                text: modelData[0]
                                flat: processes.kind !== modelData[1]
                                highlighted: processes.kind === modelData[1]
                                onClicked: {
                                    processes.kind = modelData[1]
                                    processes.load()
                                }
                            }
                        }
                    }

                    Rectangle {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        radius: 9
                        color: theme.card
                        border.color: theme.rule
                        clip: true

                        // Column heads, laid out as the rows are.
                        Row {
                            id: heads
                            x: 8
                            y: 0
                            height: 32
                            Repeater {
                                model: ["Kind", "Name", "State", "Work", "Wakeups", "Detail"]
                                delegate: Label {
                                    required property string modelData
                                    required property int index
                                    width: index === 1 ? processes.nameWidth(heads.parent.width) : processes.widths[index]
                                    height: 32
                                    verticalAlignment: Text.AlignVCenter
                                    horizontalAlignment: index === 3 || index === 4 ? Text.AlignRight : Text.AlignLeft
                                    rightPadding: index === 3 || index === 4 ? 12 : 0
                                    text: modelData
                                    color: theme.subtle
                                    font.pixelSize: 12
                                    font.bold: true
                                }
                            }
                        }
                        Rectangle { y: 32; width: parent.width; height: 1; color: theme.rule }

                        ListView {
                            anchors.fill: parent
                            anchors.topMargin: 33
                            clip: true
                            model: processes.count
                            ScrollBar.vertical: ScrollBar {}
                            delegate: Rectangle {
                                id: prow
                                required property int index
                                readonly property var modelData: processes.rows[index] || ({})
                                width: ListView.view.width
                                height: 32
                                color: rowHover.containsMouse ? theme.accentSoft
                                     : index % 2 ? theme.cardAlt : "transparent"
                                readonly property bool pickable: modelData.handle !== undefined
                                Row {
                                    x: 8
                                    height: parent.height
                                    Label {
                                        width: processes.widths[0]; height: parent.height
                                        verticalAlignment: Text.AlignVCenter
                                        text: prow.modelData.role ? prow.modelData.kind + " · " + prow.modelData.role : prow.modelData.kind
                                        color: theme.subtle
                                        elide: Text.ElideRight
                                    }
                                    Label {
                                        width: processes.nameWidth(prow.width); height: parent.height
                                        verticalAlignment: Text.AlignVCenter
                                        text: prow.modelData.name + (prow.modelData.truncated ? "…" : "")
                                        color: prow.pickable ? theme.accent : theme.text
                                        elide: Text.ElideMiddle
                                        font.bold: prow.modelData.name === "UI thread"
                                    }
                                    Item {
                                        width: processes.widths[2]; height: parent.height
                                        Rectangle {
                                            id: dot
                                            visible: prow.modelData.state !== undefined
                                            width: 8; height: 8; radius: 4
                                            anchors.verticalCenter: parent.verticalCenter
                                            color: window.stateColor(prow.modelData.state)
                                        }
                                        Label {
                                            anchors.left: dot.right
                                            anchors.leftMargin: 7
                                            anchors.verticalCenter: parent.verticalCenter
                                            text: prow.modelData.state || ""
                                            color: theme.text
                                        }
                                    }
                                    Label {
                                        width: processes.widths[3]; height: parent.height
                                        verticalAlignment: Text.AlignVCenter
                                        horizontalAlignment: Text.AlignRight
                                        rightPadding: 12
                                        text: window.ms(prow.modelData.work_us)
                                        color: theme.text
                                        font.family: theme.mono
                                    }
                                    Label {
                                        width: processes.widths[4]; height: parent.height
                                        verticalAlignment: Text.AlignVCenter
                                        horizontalAlignment: Text.AlignRight
                                        rightPadding: 12
                                        text: prow.modelData.wakeups === undefined ? "" : prow.modelData.wakeups
                                        color: theme.text
                                        font.family: theme.mono
                                    }
                                    Label {
                                        width: processes.widths[5]; height: parent.height
                                        verticalAlignment: Text.AlignVCenter
                                        elide: Text.ElideRight
                                        color: theme.subtle
                                        text: prow.modelData.kind === "timer"
                                              ? "every " + prow.modelData.interval_ms + " ms" + (prow.modelData.repeat ? ", repeats" : "")
                                              : prow.modelData.locator || prow.modelData.thread_name || ""
                                    }
                                }
                                MouseArea {
                                    id: rowHover
                                    anchors.fill: parent
                                    hoverEnabled: prow.pickable
                                    enabled: prow.pickable
                                    onClicked: {
                                        window.page = 3
                                        objects.load(true)
                                        objects.pick(prow.modelData.handle, prow.modelData.name)
                                    }
                                }
                            }
                        }
                    }
                }

                // ----- Mailboxes ---------------------------------------
                ColumnLayout {
                    id: mailboxes
                    spacing: 10
                    property var rows: []
                    // Given to the view as a number, as the processes are.
                    property int count: 0

                    function show(list) {
                        rows = list
                        count = list.length
                    }

                    function load() {
                        window.ask("app_processes", { kind: "mailbox", peek: 5 }, "mailboxes")
                    }

                    RowLayout {
                        Label { text: "Mailboxes"; color: theme.text; font.pixelSize: 19; font.bold: true }
                        Label {
                            Layout.leftMargin: 12
                            text: "counted since the inspector started · kinds only, never contents"
                            color: theme.subtle
                        }
                    }

                    GridView {
                        id: boxes
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        cellWidth: Math.max(280, width / Math.max(1, Math.floor(width / 320)))
                        cellHeight: 168
                        model: mailboxes.count
                        delegate: Item {
                            required property int index
                            readonly property var modelData: mailboxes.rows[index] || ({})
                            width: boxes.cellWidth
                            height: boxes.cellHeight
                            Rectangle {
                                anchors.fill: parent
                                anchors.margins: 6
                                radius: 9
                                color: theme.card
                                border.color: theme.rule
                                ColumnLayout {
                                    anchors.fill: parent
                                    anchors.margins: 14
                                    spacing: 7
                                    RowLayout {
                                        Label { text: modelData.name; color: theme.text; font.bold: true; font.pixelSize: 15 }
                                        Item { Layout.fillWidth: true }
                                        Label {
                                            text: modelData.depth + " / " + modelData.capacity
                                            color: theme.text
                                            font.family: theme.mono
                                        }
                                    }
                                    // How full it is.
                                    Rectangle {
                                        Layout.fillWidth: true
                                        height: 6
                                        radius: 3
                                        color: theme.rule
                                        Rectangle {
                                            height: parent.height
                                            radius: 3
                                            width: parent.width * Math.min(1, modelData.depth / Math.max(1, modelData.capacity))
                                            color: modelData.depth >= modelData.capacity ? theme.bad
                                                 : modelData.depth > 0 ? theme.warn : theme.good
                                        }
                                    }
                                    GridLayout {
                                        columns: 4
                                        columnSpacing: 12
                                        rowSpacing: 2
                                        Label { text: "sent"; color: theme.subtle }
                                        Label { text: modelData.sent; color: theme.text; font.family: theme.mono }
                                        Label { text: "drained"; color: theme.subtle }
                                        Label { text: modelData.drained; color: theme.text; font.family: theme.mono }
                                        Label { text: "refused"; color: theme.subtle }
                                        Label {
                                            text: modelData.refused
                                            font.family: theme.mono
                                            color: modelData.refused > 0 ? theme.bad : theme.text
                                        }
                                        Label { text: "replaced"; color: theme.subtle }
                                        Label { text: modelData.replaced; color: theme.text; font.family: theme.mono }
                                        Label { text: "senders"; color: theme.subtle }
                                        Label {
                                            text: modelData.senders + (modelData.waiting_senders > 0 ? " (" + modelData.waiting_senders + " waiting)" : "")
                                            color: theme.text
                                            font.family: theme.mono
                                        }
                                        Label { text: "oldest"; color: theme.subtle }
                                        Label {
                                            text: modelData.oldest_us === undefined || modelData.oldest_us === null ? "—" : window.ms(modelData.oldest_us)
                                            color: theme.text
                                            font.family: theme.mono
                                        }
                                    }
                                    Label {
                                        Layout.fillWidth: true
                                        elide: Text.ElideRight
                                        color: theme.subtle
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
                    property var tree: []
                    property string filter: ""
                    property bool truncated: false
                    property string picked: ""
                    property string pickedName: ""
                    property bool pickedItem: true
                    property var props: []
                    // The rows are kept while their number stays: a
                    // refresh changes what they say, and a row being
                    // clicked or typed in is not made again under it.
                    property int propRows: 0
                    property string head: ""
                    property string where: ""
                    property bool all: false
                    property string picture: ""
                    property string pictureNote: ""
                    // The property whose field is being typed in: the
                    // values are not shown again under it until it is
                    // done. It is the field's focus, not its active
                    // focus, which the window losing activation takes.
                    property string editing: ""
                    // The picture was asked with the properties, not
                    // after them.
                    property bool shotAsked: false

                    // The tree is asked again when a reload made new
                    // objects; a drawn pass changes values, not objects.
                    function load(tree) {
                        if (tree || objects.tree.length === 0)
                            window.ask("app_objects", { max: 3000 }, "objects")
                        if (picked !== "")
                            inspect(true)
                    }

                    function showTree(ok, s, r) {
                        if (!ok) {
                            status.text = r
                            return
                        }
                        tree = s.objects
                        truncated = s.truncated
                    }

                    function shown(row) {
                        if (filter === "")
                            return true
                        var f = filter.toLowerCase()
                        return row.type.toLowerCase().indexOf(f) >= 0
                            || (row.id || "").toLowerCase().indexOf(f) >= 0
                            || (row.object_name || "").toLowerCase().indexOf(f) >= 0
                    }

                    readonly property var rows: {
                        var out = []
                        for (var i = 0; i < tree.length; ++i)
                            if (shown(tree[i]))
                                out.push(tree[i])
                        return out
                    }

                    function pick(handle, name) {
                        editing = ""
                        picked = handle
                        pickedName = name || handle
                        inspect(false)
                    }

                    // Asks again for the one picked, leaving a field
                    // being typed in as it is. An item already known to
                    // be one has its picture asked at the same time, so
                    // that a change is shown one answer after it is made
                    // rather than two.
                    function inspect(withPicture) {
                        window.ask("app_inspect", { target: picked, all: all }, "inspect")
                        shotAsked = withPicture === true && pickedItem
                        if (shotAsked)
                            window.ask("app_screenshot", { target: picked, max_size: 640 }, "picture")
                    }

                    function showProperties(ok, s, r) {
                        if (!ok) {
                            props = []
                            propRows = 0
                            head = r
                            where = ""
                            picture = ""
                            pictureNote = ""
                            return
                        }
                        head = (s.type || "") + (s.id ? "#" + s.id : "") + (s.object_name ? "  \"" + s.object_name + "\"" : "")
                        where = s.source ? s.source.file.split("/").pop() + ":" + s.source.line : ""
                        if (editing === "") {
                            props = s.properties
                            propRows = props.length
                        }
                        pickedItem = s.geometry !== undefined && s.geometry !== null
                        if (pickedItem) {
                            if (!shotAsked)
                                window.ask("app_screenshot", { target: picked, max_size: 640 }, "picture")
                        } else {
                            picture = ""
                            pictureNote = "Not drawn: it is not an item"
                        }
                        shotAsked = false
                    }

                    function showPicture(ok, r) {
                        pictureNote = ok ? "" : r
                        if (!ok) {
                            picture = ""
                            return
                        }
                        for (var i = 0; i < r.content.length; ++i)
                            if (r.content[i].type === "image")
                                picture = "data:" + r.content[i].mimeType + ";base64," + r.content[i].data
                    }

                    // A value as a field shows it: a real to as many
                    // places as can be told apart on screen.
                    function valueText(v) {
                        if (v === null || v === undefined)
                            return ""
                        var t = String(v)
                        if (/^-?[0-9]+\.[0-9]{5,}$/.test(t))
                            return String(Math.round(Number(t) * 10000) / 10000)
                        return t
                    }

                    // The text of a field as the JSON the property takes.
                    function typed(type, text) {
                        if (type === "bool")
                            return text === "true"
                        if (["int", "real", "double", "number"].indexOf(type) >= 0 && text.trim() !== "" && !isNaN(Number(text)))
                            return Number(text)
                        return text
                    }

                    // An edit covers a binding rather than ending it, so
                    // that Revert brings the binding back.
                    function edit(name, type, value) {
                        editing = ""
                        window.ask("app_set_property",
                                   { target: picked, property: name, value: value, keep_binding: true },
                                   "edited")
                    }

                    // Text that starts with `=` is an expression the
                    // property is bound to, as a spreadsheet's formula
                    // is: it follows what it reads from then on.
                    function bind(name, expression) {
                        editing = ""
                        window.ask("app_set_property",
                                   { target: picked, property: name, binding: expression, keep_binding: true },
                                   "edited")
                    }

                    function revert(name) {
                        window.ask("app_set_property", { target: picked, property: name, revert: true }, "edited")
                    }

                    function edited(ok, r) {
                        if (ok)
                            inspect(true)
                    }

                    // The tree.
                    Rectangle {
                        Layout.preferredWidth: 300
                        Layout.fillHeight: true
                        radius: 9
                        color: theme.card
                        border.color: theme.rule
                        clip: true

                        TextField {
                            id: treeFilter
                            x: 8; y: 8
                            width: parent.width - 16
                            placeholderText: "Filter by type, id or name"
                            onTextChanged: objects.filter = text
                        }
                        Label {
                            x: 12
                            anchors.top: treeFilter.bottom
                            anchors.topMargin: 6
                            color: theme.subtle
                            font.pixelSize: 11
                            text: objects.rows.length + " of " + objects.tree.length + " objects"
                                  + (objects.truncated ? " (more than shown)" : "")
                        }
                        ListView {
                            id: treeView
                            anchors.fill: parent
                            anchors.topMargin: treeFilter.height + 34
                            clip: true
                            model: objects.rows
                            ScrollBar.vertical: ScrollBar {}
                            delegate: Rectangle {
                                id: node
                                required property var modelData
                                width: ListView.view.width
                                height: 26
                                color: objects.picked === modelData.handle ? theme.accentSoft
                                     : nodeHover.containsMouse ? Qt.rgba(theme.text.r, theme.text.g, theme.text.b, 0.05)
                                     : "transparent"
                                // Text, not Label, as in the property table:
                                // rows are made as a tree of thousands is
                                // scrolled, and a Text is cheaper to make.
                                Row {
                                    x: 10 + (objects.filter === "" ? node.modelData.depth * 14 : 0)
                                    height: parent.height
                                    spacing: 6
                                    Rectangle {
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: 6; height: 6; radius: node.modelData.item ? 1 : 3
                                        color: !node.modelData.item ? theme.violet
                                             : node.modelData.visible === false ? theme.subtle : theme.accent
                                    }
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: node.modelData.type
                                        color: node.modelData.visible === false ? theme.subtle : theme.text
                                        font.bold: objects.picked === node.modelData.handle
                                    }
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: text !== ""
                                        text: node.modelData.id ? "#" + node.modelData.id
                                            : node.modelData.object_name ? "\"" + node.modelData.object_name + "\"" : ""
                                        color: theme.accent
                                    }
                                    // Without an id or a name, the handle tells
                                    // one Timer from the next.
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: !node.modelData.id && !node.modelData.object_name
                                        text: node.modelData.handle
                                        color: theme.subtle
                                        font.pixelSize: 11
                                    }
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: node.modelData.hidden !== undefined
                                        text: "+" + node.modelData.hidden
                                        color: theme.subtle
                                    }
                                }
                                MouseArea {
                                    id: nodeHover
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    onClicked: objects.pick(node.modelData.handle,
                                                            node.modelData.type + (node.modelData.id ? "#" + node.modelData.id : ""))
                                }
                            }
                        }
                    }

                    // The properties of the one picked.
                    Rectangle {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        radius: 9
                        color: theme.card
                        border.color: theme.rule
                        clip: true

                        ColumnLayout {
                            anchors.fill: parent
                            anchors.margins: 12
                            spacing: 6

                            RowLayout {
                                Layout.fillWidth: true
                                Label {
                                    Layout.fillWidth: true
                                    text: objects.picked === "" ? "Pick an object" : objects.head
                                    color: theme.text
                                    font.bold: true
                                    font.pixelSize: 15
                                    elide: Text.ElideRight
                                }
                                Button {
                                    visible: objects.picked !== ""
                                    text: trace.traces(objects.picked) ? "Stop tracing" : "Trace"
                                    onClicked: trace.toggle(objects.picked)
                                }
                                CheckBox {
                                    text: "Defaults too"
                                    checked: objects.all
                                    onToggled: {
                                        objects.all = checked
                                        if (objects.picked !== "")
                                            objects.pick(objects.picked, objects.pickedName)
                                    }
                                }
                            }
                            Label {
                                visible: objects.picked !== ""
                                text: objects.picked + (objects.where === "" ? "" : "  ·  " + objects.where)
                                      + "  ·  =expression binds"
                                color: theme.subtle
                                font.family: theme.mono
                                font.pixelSize: 11
                            }
                            Rectangle { Layout.fillWidth: true; height: 1; color: theme.rule }

                            ListView {
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                clip: true
                                model: objects.propRows
                                ScrollBar.vertical: ScrollBar {}
                                delegate: Item {
                                    id: prop
                                    required property int index
                                    readonly property var modelData: objects.props[index] || ({})
                                    width: ListView.view.width
                                    height: 36
                                    readonly property bool edited: modelData.assigned_by === "devtools"
                                    readonly property bool fixed: modelData.read_only === true
                                    readonly property bool boolean: modelData.type === "bool"
                                    readonly property bool held: modelData.items !== undefined || modelData.handle !== undefined
                                    readonly property bool editable: !fixed && !boolean && !held
                                    // A row is text and frames until its value
                                    // is typed in: a field is made for that and
                                    // let go after, as a screenful of fields
                                    // costs more to make than a frame has.
                                    property bool typing: false
                                    Rectangle {
                                        anchors.fill: parent
                                        color: prop.index % 2 ? theme.cardAlt : "transparent"
                                    }
                                    Text {
                                        id: propName
                                        x: 6
                                        width: 130
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: prop.modelData.name
                                        color: theme.text
                                        elide: Text.ElideRight
                                    }
                                    Text {
                                        id: propType
                                        anchors.left: propName.right
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: 60
                                        text: prop.modelData.type
                                        color: theme.subtle
                                        font.pixelSize: 11
                                        elide: Text.ElideRight
                                    }
                                    // A value that can be changed is framed, so
                                    // it reads apart from one that cannot.
                                    Rectangle {
                                        id: valueBox
                                        anchors.left: propType.right
                                        anchors.right: chips.left
                                        anchors.rightMargin: 8
                                        anchors.verticalCenter: parent.verticalCenter
                                        height: 28
                                        radius: 5
                                        color: "transparent"
                                        border.color: !prop.editable ? "transparent"
                                            : valueArea.containsMouse ? theme.subtle : theme.rule
                                        Text {
                                            anchors.left: parent.left
                                            anchors.leftMargin: prop.editable ? 6 : 0
                                            anchors.right: parent.right
                                            anchors.rightMargin: 6
                                            anchors.verticalCenter: parent.verticalCenter
                                            visible: !prop.typing && !prop.boolean
                                            text: prop.modelData.value === null && !prop.editable ? "—"
                                                : objects.valueText(prop.modelData.value)
                                            color: prop.modelData.handle !== undefined ? theme.accent : theme.text
                                            elide: Text.ElideRight
                                        }
                                        // A check for a value that is true or false.
                                        Loader {
                                            active: prop.boolean
                                            anchors.verticalCenter: parent.verticalCenter
                                            sourceComponent: check
                                        }
                                        Component {
                                            id: check
                                            Row {
                                                spacing: 8
                                                Accessible.role: Accessible.CheckBox
                                                Accessible.name: prop.modelData.name || ""
                                                Accessible.checked: prop.modelData.value === "true"
                                                Rectangle {
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    width: 16; height: 16; radius: 4
                                                    color: prop.modelData.value === "true" && !prop.fixed ? theme.accent : "transparent"
                                                    border.color: prop.modelData.value === "true" && !prop.fixed ? theme.accent
                                                        : valueArea.containsMouse ? theme.subtle : theme.rule
                                                    Text {
                                                        anchors.centerIn: parent
                                                        visible: prop.modelData.value === "true"
                                                        text: "✓"
                                                        color: prop.fixed ? theme.subtle : theme.bg
                                                        font.pixelSize: 12
                                                        font.bold: true
                                                    }
                                                }
                                                Text {
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    text: prop.modelData.value === null ? "" : String(prop.modelData.value)
                                                    color: theme.text
                                                }
                                            }
                                        }
                                        MouseArea {
                                            id: valueArea
                                            anchors.fill: parent
                                            enabled: !prop.typing && (prop.editable || prop.modelData.handle !== undefined
                                                                       || (prop.boolean && !prop.fixed))
                                            hoverEnabled: enabled
                                            cursorShape: prop.editable ? Qt.IBeamCursor : Qt.PointingHandCursor
                                            onClicked: {
                                                if (prop.modelData.handle !== undefined)
                                                    objects.pick(prop.modelData.handle, prop.modelData.value)
                                                else if (prop.boolean)
                                                    objects.edit(prop.modelData.name, "bool", prop.modelData.value !== "true")
                                                else
                                                    prop.typing = true
                                            }
                                        }
                                        Loader {
                                            anchors.fill: parent
                                            active: prop.typing
                                            // Focus is given once the click that
                                            // made the field is over.
                                            onLoaded: {
                                                objects.editing = prop.modelData.name
                                                Qt.callLater(function() {
                                                    if (!item)
                                                        return
                                                    item.forceActiveFocus()
                                                    item.selectAll()
                                                })
                                            }
                                            sourceComponent: TextField {
                                                id: field
                                                topPadding: 4
                                                bottomPadding: 4
                                                leftPadding: 6
                                                clip: true
                                                color: theme.text
                                                background: Rectangle {
                                                    radius: 5
                                                    color: theme.bg
                                                    border.color: theme.accent
                                                }
                                                // A bound value is typed in as the
                                                // expression it is bound to.
                                                text: prop.modelData.expression !== undefined
                                                    ? "=" + prop.modelData.expression
                                                    : objects.valueText(prop.modelData.value)
                                                // Its focus, not its active focus,
                                                // which the window losing activation
                                                // takes.
                                                onFocusChanged: if (!focus) prop.done()
                                                onAccepted: {
                                                    var name = prop.modelData.name
                                                    var typed = text
                                                    prop.done()
                                                    if (typed.charAt(0) === "=")
                                                        objects.bind(name, typed.substring(1))
                                                    else
                                                        objects.edit(name, prop.modelData.type,
                                                                     objects.typed(prop.modelData.type, typed))
                                                }
                                                Keys.onEscapePressed: prop.done()
                                            }
                                        }
                                    }
                                    // The field is let go once its handler is over.
                                    function done() {
                                        if (objects.editing === modelData.name)
                                            objects.editing = ""
                                        Qt.callLater(function() { prop.typing = false })
                                    }
                                    Row {
                                        id: chips
                                        anchors.right: parent.right
                                        anchors.rightMargin: 4
                                        anchors.verticalCenter: parent.verticalCenter
                                        spacing: 6
                                        Rectangle {
                                            height: 20
                                            width: chipText.implicitWidth + 14
                                            radius: 10
                                            anchors.verticalCenter: parent.verticalCenter
                                            readonly property color tone: prop.edited ? theme.warn
                                                : prop.modelData.provenance === "bound" ? theme.accent
                                                : prop.modelData.provenance === "animated" ? theme.violet
                                                : theme.subtle
                                            color: Qt.rgba(tone.r, tone.g, tone.b, 0.16)
                                            Text {
                                                id: chipText
                                                anchors.centerIn: parent
                                                text: prop.edited ? (prop.modelData.provenance === "bound" ? "rebound" : "edited")
                                                        + (prop.modelData.binding_kept ? " · bound below" : "")
                                                    : prop.fixed ? prop.modelData.provenance + " · read-only"
                                                    : prop.modelData.provenance
                                                color: parent.tone
                                                font.pixelSize: 11
                                            }
                                        }
                                        Text {
                                            visible: prop.edited
                                            anchors.verticalCenter: parent.verticalCenter
                                            text: "Revert"
                                            color: theme.accent
                                            font.pixelSize: 12
                                            font.underline: revertArea.containsMouse
                                            MouseArea {
                                                id: revertArea
                                                anchors.fill: parent
                                                anchors.margins: -4
                                                enabled: prop.edited
                                                hoverEnabled: enabled
                                                cursorShape: Qt.PointingHandCursor
                                                onClicked: objects.revert(prop.modelData.name)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // A picture of it, drawn again when the application
                    // draws.
                    Rectangle {
                        id: frame
                        Layout.preferredWidth: 280
                        Layout.fillHeight: true
                        radius: 9
                        color: theme.card
                        border.color: theme.rule
                        // Two pictures take turns: the next one loads
                        // under the one shown and comes to the top once
                        // it is ready, so that a picture that keeps
                        // changing never blanks while it loads.
                        property int front: 0
                        readonly property Image shown: front === 0 ? shotA : shotB
                        readonly property Image back: front === 0 ? shotB : shotA

                        function show(url) {
                            if (url === "") {
                                shotA.source = ""
                                shotB.source = ""
                                return
                            }
                            back.asked = url
                            back.source = url
                            // A picture already decoded is ready without
                            // its status changing.
                            Qt.callLater(frame.loaded, back)
                        }
                        function loaded(which) {
                            if (which === back && which.status === Image.Ready
                                    && which.asked === objects.picture)
                                front = 1 - front
                        }

                        Connections {
                            target: objects
                            function onPictureChanged() { frame.show(objects.picture) }
                        }
                        // An outline, so that a white picture shows on a
                        // white card.
                        Rectangle {
                            anchors.centerIn: parent
                            visible: objects.picture !== "" && frame.shown.status === Image.Ready
                            width: frame.shown.paintedWidth + 2
                            height: frame.shown.paintedHeight + 2
                            color: "transparent"
                            border.color: theme.rule
                        }
                        Image {
                            id: shotA
                            property string asked: ""
                            anchors.centerIn: parent
                            z: frame.front === 0 ? 1 : 0
                            opacity: frame.front === 0 ? 1 : 0
                            width: Math.min(implicitWidth, frame.width - 24)
                            height: Math.min(implicitHeight, frame.height - 24)
                            fillMode: Image.PreserveAspectFit
                            onStatusChanged: frame.loaded(shotA)
                        }
                        Image {
                            id: shotB
                            property string asked: ""
                            anchors.centerIn: parent
                            z: frame.front === 1 ? 1 : 0
                            opacity: frame.front === 1 ? 1 : 0
                            width: Math.min(implicitWidth, frame.width - 24)
                            height: Math.min(implicitHeight, frame.height - 24)
                            fillMode: Image.PreserveAspectFit
                            onStatusChanged: frame.loaded(shotB)
                        }
                        Label {
                            anchors.centerIn: parent
                            width: parent.width - 24
                            wrapMode: Text.Wrap
                            horizontalAlignment: Text.AlignHCenter
                            visible: objects.picture === ""
                            color: theme.subtle
                            text: objects.pictureNote !== "" ? objects.pictureNote : "No picture"
                        }
                    }
                }

                // ----- Logs --------------------------------------------
                ColumnLayout {
                    id: logs
                    spacing: 10
                    property var entries: []
                    property var since: null
                    property int unread: 0
                    property string level: "debug"
                    property string filter: ""
                    readonly property var levels: ["debug", "info", "warning", "critical"]

                    // Only what is newer than what the window has.
                    function load() {
                        var args = { limit: 100 }
                        if (since !== null)
                            args.since = since
                        window.ask("app_logs", args, "logs")
                    }

                    function add(s) {
                        since = s.next_since
                        if (s.entries.length === 0)
                            return
                        var all = entries.concat(s.entries)
                        var over = Math.max(0, all.length - 2000)
                        entries = all.slice(over)
                        if (over > 0) {
                            show()
                        } else {
                            for (var i = 0; i < s.entries.length; ++i)
                                put(s.entries[i])
                            // Once for the lot: at each line, the view
                            // would make the new row and drop the top one.
                            logView.positionViewAtEnd()
                        }
                        if (window.page !== 4)
                            unread += s.entries.length
                    }

                    function matches(e) {
                        if (levels.indexOf(e.level) < levels.indexOf(level))
                            return false
                        var f = filter.toLowerCase()
                        return f === "" || e.message.toLowerCase().indexOf(f) >= 0
                            || (e.category || "").toLowerCase().indexOf(f) >= 0
                    }

                    // Lines are added to the model, not given as a new
                    // array: a view given a new array makes every row
                    // again, and a busy application logs all the time.
                    function put(e) {
                        if (!matches(e))
                            return
                        shownLines.append({
                            level: e.level,
                            category: e.category || "",
                            message: e.message,
                            where: e.file ? e.file.split("/").pop() + ":" + e.line : ""
                        })
                    }

                    function show() {
                        shownLines.clear()
                        for (var i = 0; i < entries.length; ++i)
                            put(entries[i])
                        logView.positionViewAtEnd()
                    }

                    onFilterChanged: show()
                    onLevelChanged: show()

                    ListModel { id: shownLines }

                    function tone(level) {
                        return level === "critical" ? theme.bad
                             : level === "warning" ? theme.warn
                             : level === "info" ? theme.accent : theme.subtle
                    }

                    Connections {
                        target: window
                        function onPageChanged() {
                            if (window.page === 4)
                                logs.unread = 0
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 10
                        Label { text: "Logs"; color: theme.text; font.pixelSize: 19; font.bold: true }
                        Label {
                            text: shownLines.count + " of " + logs.entries.length + " lines"
                            color: theme.subtle
                        }
                        Item { Layout.fillWidth: true }
                        TextField {
                            Layout.preferredWidth: 260
                            placeholderText: "Filter text or category"
                            onTextChanged: logs.filter = text
                        }
                        ComboBox {
                            model: ["debug and up", "info and up", "warnings and up", "critical only"]
                            onActivated: function(index) { logs.level = logs.levels[index] }
                        }
                        Button {
                            text: "Clear"
                            onClicked: {
                                logs.entries = []
                                shownLines.clear()
                            }
                        }
                    }

                    Rectangle {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        radius: 9
                        color: theme.card
                        border.color: theme.rule
                        clip: true
                        ListView {
                            id: logView
                            anchors.fill: parent
                            anchors.margins: 6
                            clip: true
                            model: shownLines
                            ScrollBar.vertical: ScrollBar {}
                            delegate: Item {
                                id: entry
                                required property int index
                                required property string level
                                required property string category
                                required property string message
                                required property string where
                                width: ListView.view.width
                                height: 24
                                Rectangle {
                                    anchors.fill: parent
                                    color: entry.index % 2 ? theme.cardAlt : "transparent"
                                }
                                Row {
                                    x: 6
                                    height: parent.height
                                    spacing: 10
                                    Label {
                                        width: 64
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: entry.level
                                        color: logs.tone(entry.level)
                                        font.bold: entry.level === "warning" || entry.level === "critical"
                                        font.pixelSize: 12
                                    }
                                    Label {
                                        width: 110
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: entry.category
                                        color: theme.subtle
                                        elide: Text.ElideRight
                                        font.pixelSize: 12
                                    }
                                    Label {
                                        width: entry.width - 64 - 110 - 160 - 50
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: entry.message
                                        color: theme.text
                                        elide: Text.ElideRight
                                        font.family: theme.mono
                                        font.pixelSize: 12
                                    }
                                    Label {
                                        width: 160
                                        anchors.verticalCenter: parent.verticalCenter
                                        horizontalAlignment: Text.AlignRight
                                        text: entry.where
                                        color: theme.subtle
                                        elide: Text.ElideLeft
                                        font.pixelSize: 11
                                    }
                                }
                            }
                        }
                        Label {
                            anchors.centerIn: parent
                            visible: shownLines.count === 0
                            text: logs.entries.length === 0 ? "Nothing logged yet" : "No line matches"
                            color: theme.subtle
                        }
                    }
                }

                // ----- Trace -------------------------------------------
                // What the traced objects were told, in order, with the
                // application's time, as Erlang's observer traces the
                // messages of a process.
                ColumnLayout {
                    id: trace
                    spacing: 10
                    property var since: null
                    property var tracing: []
                    property int dropped: 0
                    property int unread: 0

                    function args() {
                        var a = { max: 500 }
                        if (since !== null)
                            a.since = since
                        return a
                    }

                    function load() {
                        window.ask("app_trace", args(), "trace")
                    }

                    function traces(handle) {
                        for (var i = 0; i < tracing.length; ++i)
                            if (tracing[i].handle === handle)
                                return true
                        return false
                    }

                    function toggle(handle) {
                        var a = args()
                        if (handle !== undefined) {
                            a.target = handle
                            a.stop = traces(handle)
                        } else
                            a.stop = true
                        window.ask("app_trace", a, "trace")
                    }

                    function add(s) {
                        tracing = s.tracing
                        dropped = s.dropped
                        since = s.next_since
                        for (var i = 0; i < s.events.length; ++i)
                            put(s.events[i])
                        var over = traceLines.count - 2000
                        if (over > 0)
                            traceLines.remove(0, over)
                        if (s.events.length > 0) {
                            traceView.positionViewAtEnd()
                            if (window.page !== 5)
                                unread += s.events.length
                        }
                        if (s.more)
                            load()
                    }

                    function put(e) {
                        var what = e.kind === "change" ? JSON.stringify(e.value)
                                 : e.kind === "signal" ? "(" + e.args.map(function(a) { return JSON.stringify(a) }).join(", ") + ")"
                                 : ""
                        traceLines.append({
                            seq: e.seq,
                            at: (e.at_us / 1000000).toFixed(3) + " s",
                            object: e.object,
                            kind: e.kind,
                            name: e.name || "",
                            what: what
                        })
                    }

                    function tone(kind) {
                        return kind === "destroyed" ? theme.bad
                             : kind === "signal" ? theme.warn : theme.accent
                    }

                    ListModel { id: traceLines }

                    Connections {
                        target: window
                        function onPageChanged() {
                            if (window.page === 5)
                                trace.unread = 0
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 10
                        Label { text: "Trace"; color: theme.text; font.pixelSize: 19; font.bold: true }
                        Label {
                            text: traceLines.count + " events"
                                  + (trace.dropped > 0 ? " · " + trace.dropped + " older gave way" : "")
                            color: theme.subtle
                        }
                        Item { Layout.fillWidth: true }
                        Button {
                            text: "Stop all"
                            enabled: trace.tracing.length > 0
                            onClicked: trace.toggle()
                        }
                        Button {
                            text: "Clear"
                            onClicked: traceLines.clear()
                        }
                    }

                    // Whom the trace follows: Stop on each.
                    Flow {
                        Layout.fillWidth: true
                        spacing: 6
                        Label {
                            visible: trace.tracing.length === 0
                            text: "Nothing is traced. Pick an object on the Objects page and press Trace."
                            color: theme.subtle
                        }
                        Repeater {
                            model: trace.tracing
                            delegate: Rectangle {
                                id: chip
                                required property var modelData
                                width: chipRow.implicitWidth + 16
                                height: 28
                                radius: 14
                                color: theme.accentSoft
                                Row {
                                    id: chipRow
                                    x: 8
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: 6
                                    Label {
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: chip.modelData.name
                                        color: theme.accent
                                        font.pixelSize: 12
                                    }
                                    Label {
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: "✕"
                                        color: theme.subtle
                                        font.pixelSize: 12
                                        MouseArea {
                                            anchors.fill: parent
                                            anchors.margins: -4
                                            Accessible.role: Accessible.Button
                                            Accessible.name: "Stop tracing " + chip.modelData.name
                                            onClicked: trace.toggle(chip.modelData.handle)
                                        }
                                    }
                                }
                            }
                        }
                    }

                    Rectangle {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        radius: 9
                        color: theme.card
                        border.color: theme.rule
                        clip: true
                        ListView {
                            id: traceView
                            anchors.fill: parent
                            anchors.margins: 6
                            clip: true
                            model: traceLines
                            ScrollBar.vertical: ScrollBar {}
                            delegate: Item {
                                id: traceRow
                                required property int index
                                required property int seq
                                required property string at
                                required property string object
                                required property string kind
                                required property string name
                                required property string what
                                width: ListView.view.width
                                height: 24
                                Rectangle {
                                    anchors.fill: parent
                                    color: traceRow.index % 2 ? theme.cardAlt : "transparent"
                                }
                                Row {
                                    x: 6
                                    height: parent.height
                                    spacing: 10
                                    Label {
                                        width: 80
                                        anchors.verticalCenter: parent.verticalCenter
                                        horizontalAlignment: Text.AlignRight
                                        text: traceRow.at
                                        color: theme.subtle
                                        font.family: theme.mono
                                        font.pixelSize: 12
                                    }
                                    Label {
                                        width: 180
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: traceRow.object
                                        color: theme.text
                                        elide: Text.ElideRight
                                        font.pixelSize: 12
                                    }
                                    Label {
                                        width: 70
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: traceRow.kind
                                        color: trace.tone(traceRow.kind)
                                        font.pixelSize: 12
                                    }
                                    Label {
                                        width: 150
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: traceRow.name
                                        color: theme.text
                                        font.bold: true
                                        elide: Text.ElideRight
                                        font.pixelSize: 12
                                    }
                                    Label {
                                        width: traceRow.width - 80 - 180 - 70 - 150 - 50
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: traceRow.what
                                        color: theme.text
                                        elide: Text.ElideRight
                                        font.family: theme.mono
                                        font.pixelSize: 12
                                    }
                                }
                            }
                        }
                        Label {
                            anchors.centerIn: parent
                            visible: traceLines.count === 0
                            text: "Nothing written down yet"
                            color: theme.subtle
                        }
                    }
                }
            }

            // What the last refusal or failure said.
            Label {
                id: status
                Layout.fillWidth: true
                Layout.leftMargin: 18
                Layout.rightMargin: 18
                Layout.bottomMargin: 10
                elide: Text.ElideRight
                color: theme.bad
                visible: text !== ""
            }
        }
    }
}
