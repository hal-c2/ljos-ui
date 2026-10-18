// The window of `ljos observe` (crates/ljos_cli/src/observe.rs). Every
// value it shows comes from an inspector tool through `client.call`.
//
// It follows the application with `app_watch`, which the inspector
// answers on its server's thread when a pass is drawn, a line is logged or
// a reload happens, and holds otherwise: while the application rests,
// neither it nor this window does any work. Each answer brings the page
// shown up to date and adds a point to the charts.
//
// An inspector is to cost less than what it watches: words are Text, not
// Label, each in the theme's colour (a Label costs over twice a Text to
// make and hold), and a page out of sight lets the rows of its view go.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes
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

    // A value as the inspector gives it, with every digit it has: an edit
    // of 736.0003 must not read 736. The Objects and Trace pages both
    // show values through this.
    function shown(v) {
        if (v === null || v === undefined)
            return ""
        if (typeof v === "string")
            return v
        if (typeof v === "number" || typeof v === "boolean")
            return String(v)
        return JSON.stringify(v)
    }

    // What a refusal says, without the JSON it came in.
    function refusal(result) {
        var text = String(result)
        try {
            var r = JSON.parse(text)
            var e = r && r.error ? r.error : r
            if (e && typeof e.message === "string")
                return e.message
        } catch (err) {
        }
        return text
    }

    // ----- Asking -----------------------------------------------------

    // What each answer still awaited is for, by serial: the name of a
    // handler in `answered` below, as a property holds values, not
    // functions.
    property var waiting: ({})

    // The name the traces started here are held by: the inspector stops
    // them once this window no longer watches the application, as when
    // it is closed.
    readonly property string holder: "observer " + Date.now().toString(36)
        + Math.floor(Math.random() * 1e9).toString(36)

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
        else if (route.indexOf("objects:") === 0)
            objects.showTree(ok, s, r, parseInt(route.substring(8)), false)
        else if (route.indexOf("objects+:") === 0)
            objects.showTree(ok, s, r, parseInt(route.substring(9)), true)
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
                if (route && route.indexOf("objects") === 0)
                    objects.turnedAway(parseInt(route.substring(route.indexOf(":") + 1)))
                return
            }
            // A picture that cannot be taken is said where the picture
            // would be.
            if (route !== "watch" && route !== "picture")
                status.text = ok ? "" : tool + ": " + window.refusal(result)
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

        // Paused, it still watches, so that the traces it holds stay, and
        // shows nothing until followed again.
        property bool missed: false

        function start() {
            if (waiting)
                return
            waiting = true
            var args = counts === null ? {}
                : { since: { paints: counts.paints, logs: counts.logs, reloads: counts.reloads,
                            traces: counts.traces },
                    wait_ms: 30000, settle_ms: 1000, holder: window.holder }
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
            if (!live) {
                missed = missed || first || s.changed.length > 0
                start()
                return
            }
            if (first)
                window.refresh()
            else if (s.changed.length > 0) {
                quietSince = ""
                window.refresh(s.changed)
            } else if (quietSince === "")
                quietSince = Qt.formatTime(new Date(), "hh:mm:ss")
            start()
        }

        onLiveChanged: {
            if (live && missed) {
                missed = false
                window.refresh()
            }
        }
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

                Text {
                    text: "Observer"
                    color: theme.text
                    font.pixelSize: 21
                    font.bold: true
                }
                Text {
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
                        Text {
                            anchors.left: parent.left
                            anchors.leftMargin: 14
                            anchors.verticalCenter: parent.verticalCenter
                            text: modelData
                            color: window.page === index ? theme.accent : theme.text
                            font.bold: window.page === index
                        }
                        Text {
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
                            Accessible.role: Accessible.PageTab
                            Accessible.name: modelData
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
                    Text {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        text: !client.connected ? "not connected"
                            : !watcher.live ? "paused"
                            : watcher.quietSince !== "" ? "idle since " + watcher.quietSince
                            : "live"
                        color: theme.subtle
                    }
                }
                Text {
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
                    // the point before, kept as one list per chart so
                    // that a chart reads its own without going through
                    // every point.
                    property var series: ({ frameMs: [], drawn: [], skipped: [], rss: [], wakeups: [], inbox: [] })
                    property int version: 0
                    property var last: null
                    property var parts: ({})
                    property var latest: ({})

                    function sample() {
                        parts = {}
                        window.ask("app_metrics", {}, "sample-metrics")
                    }

                    function part(name, s) {
                        var next = parts
                        next[name] = s
                        parts = next
                        // The threads are asked once the metrics are
                        // answered. Their mailboxes are counted after the
                        // UI thread answered, so that the depth is not
                        // this window's own questions waiting.
                        if (name === "metrics")
                            window.ask("app_processes", {}, "sample-threads")
                        else if (parts.metrics)
                            add(parts.metrics, s, s)
                    }

                    function add(m, t, b) {
                        if (t.process)
                            processes.process = t.process
                        var now = Date.now()
                        var wakeups = 0
                        var threadCount = 0
                        for (var i = 0; i < t.processes.length; ++i) {
                            if (t.processes[i].kind !== "thread")
                                continue
                            wakeups += t.processes[i].wakeups || 0
                            threadCount += 1
                        }
                        var depth = 0
                        var boxes = b.mailboxes || []
                        for (var j = 0; j < boxes.length; ++j)
                            depth += boxes[j].depth
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
                            threads: t.process ? t.process.threads : threadCount,
                            rss: t.process ? t.process.rss_bytes : null,
                            drawn: here.drawn,
                            skipped: here.skipped,
                            painter: p.painter || "",
                        }
                        var next = {}
                        for (var key in point) {
                            var was = series[key]
                            var kept = was.slice(Math.max(0, was.length - 119))
                            kept.push(point[key])
                            next[key] = kept
                        }
                        series = next
                        version += 1
                        // The count comes after the change that made
                        // the objects, and an application at rest tells
                        // of no other.
                        if (window.page === 3)
                            objects.follow(false)
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        Text { text: "Overview"; color: theme.text; font.pixelSize: 19; font.bold: true }
                        Item { Layout.fillWidth: true }
                        Text {
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
                                    Text {
                                        text: modelData[0]
                                        color: theme.subtle
                                        font.pixelSize: 11
                                    }
                                    Text {
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

                                readonly property var values: overview.series[modelData.key]
                                readonly property var seconds: modelData.second ? overview.series[modelData.second] : []
                                readonly property real now: values.length === 0 ? 0 : values[values.length - 1]

                                Text {
                                    x: 12; y: 10
                                    text: chart.modelData.title
                                    color: theme.subtle
                                }
                                Text {
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
                                // Lines, not a canvas: a canvas keeps a
                                // picture of itself the size of the chart and
                                // draws its text through the canvas's own
                                // path, which cost more memory than the rest
                                // of the page and took the first frame over
                                // its budget.
                                Item {
                                    id: plot
                                    anchors.fill: parent
                                    anchors.topMargin: 34
                                    anchors.margins: 12
                                    // Each line its own property: a sample
                                    // then makes one array of points per line
                                    // and nothing in between.
                                    readonly property real top: scaleOf(chart.values, chart.seconds)
                                    readonly property var line: points(chart.values, top, width, height)
                                    readonly property var second: points(chart.seconds, top, width, height)
                                    // The line taken down to the bottom,
                                    // filled under it. Empty for less than
                                    // two points, which enclose nothing.
                                    readonly property var area: line.length < 2 ? []
                                        : line.concat([Qt.point(width, height), Qt.point(line[0].x, height)])

                                    function scaleOf(a, b) {
                                        var top = 0
                                        for (var i = 0; i < a.length; ++i)
                                            top = Math.max(top, a[i])
                                        for (var k = 0; k < b.length; ++k)
                                            top = Math.max(top, b[k])
                                        return top <= 0 ? 1 : top * 1.15
                                    }
                                    function points(vs, top, w, h) {
                                        var step = w / 119
                                        var x0 = w - (vs.length - 1) * step
                                        var out = []
                                        for (var n = 0; n < vs.length; ++n)
                                            out.push(Qt.point(x0 + n * step, h - vs[n] / top * h))
                                        return out
                                    }

                                    Rectangle { width: plot.width; height: 1; color: theme.rule }
                                    Rectangle { y: Math.round(plot.height / 2); width: plot.width; height: 1; color: theme.rule }
                                    Rectangle { y: plot.height - 1; width: plot.width; height: 1; color: theme.rule }
                                    Shape {
                                        anchors.fill: parent
                                        ShapePath {
                                            strokeWidth: -1
                                            fillColor: Qt.rgba(chart.tone.r, chart.tone.g, chart.tone.b, 0.14)
                                            PathPolyline { path: plot.area }
                                        }
                                        ShapePath {
                                            strokeColor: theme.accent
                                            strokeWidth: 2
                                            fillColor: "transparent"
                                            PathPolyline { path: plot.second }
                                        }
                                        ShapePath {
                                            strokeColor: chart.tone
                                            strokeWidth: 2
                                            fillColor: "transparent"
                                            PathPolyline { path: plot.line }
                                        }
                                    }
                                    // A single point is no line.
                                    Rectangle {
                                        visible: chart.values.length === 1
                                        x: plot.width - 2
                                        y: plot.line.length === 1 ? plot.line[0].y - 2 : 0
                                        width: 4; height: 4
                                        color: chart.tone
                                    }
                                    Text {
                                        x: 2
                                        visible: chart.values.length > 0
                                        text: String(Math.round(plot.top * 10) / 10)
                                        color: theme.subtle
                                        font.pixelSize: 10
                                    }
                                    Text {
                                        x: 4
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: chart.values.length === 0
                                        text: "waiting for the application to change"
                                        color: theme.subtle
                                        font.pixelSize: 12
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
                    // A busy application is shown anew each second. The
                    // view's rows are a ListModel set row by row from the
                    // answer (`held` is what it holds): a row whose values
                    // are the same is left alone, and a count that changes
                    // adds or takes rows at the end, never all of them.
                    ListModel { id: procRows }
                    property var held: []
                    // A row made costs a delegate, so rows are set a piece
                    // at a time, as long as `applyMs` allows a frame:
                    // `incoming` from `at` on is still to come.
                    property var incoming: []
                    property int at: 0
                    readonly property int applyMs: 2
                    Timer { id: applying; interval: 1; repeat: true; onTriggered: processes.step() }
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
                        incoming = s.processes
                        at = 0
                        if (s.process)
                            process = s.process
                        step()
                    }

                    function step() {
                        var rows = incoming
                        var h = held
                        var began = Date.now()
                        var i = at
                        for (; i < rows.length; ++i) {
                            if (Date.now() - began >= applyMs)
                                break
                            var row = flat(rows[i])
                            if (i >= h.length) {
                                procRows.append(row)
                                h.push(row)
                            } else if (differs(h[i], row)) {
                                procRows.set(i, row)
                                h[i] = row
                            }
                        }
                        at = i
                        held = h
                        if (i < rows.length) {
                            if (!applying.running)
                                applying.start()
                            return
                        }
                        applying.stop()
                        if (h.length > rows.length) {
                            procRows.remove(rows.length, h.length - rows.length)
                            h.splice(rows.length, h.length - rows.length)
                        }
                    }

                    // A row as the view's model holds it: what the
                    // columns show, worked out once.
                    function flat(r) {
                        return {
                            what: r.role ? r.kind + " · " + r.role : r.kind || "",
                            name: r.name || "",
                            truncated: r.truncated === true,
                            status: r.state || "",
                            hasStatus: r.state !== undefined,
                            work: window.ms(r.work_us),
                            wakeups: r.wakeups === undefined ? "" : String(r.wakeups),
                            detail: r.kind === "timer"
                                    ? "every " + r.interval_ms + " ms" + (r.repeat ? ", repeats" : "")
                                    : r.locator || r.thread_name || "",
                            handle: r.handle === undefined ? "" : r.handle,
                            pickable: r.handle !== undefined
                        }
                    }

                    function differs(a, b) {
                        for (var k in b) {
                            if (a[k] !== b[k])
                                return true
                        }
                        return false
                    }

                    function nameWidth(total) {
                        var rest = 0
                        for (var i = 0; i < widths.length; ++i)
                            rest += widths[i]
                        return Math.max(120, total - rest - 16)
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        Text { text: "Processes"; color: theme.text; font.pixelSize: 19; font.bold: true }
                        Text {
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
                                delegate: Text {
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
                            // A page out of sight lets its rows go: they
                            // are most of what a page costs to hold.
                            model: window.page === 1 ? procRows : null
                            ScrollBar.vertical: ScrollBar {}
                            delegate: Rectangle {
                                id: prow
                                required property int index
                                required property string what
                                required property string name
                                required property bool truncated
                                required property string status
                                required property bool hasStatus
                                required property string work
                                required property string wakeups
                                required property string detail
                                required property string handle
                                required property bool pickable
                                width: ListView.view.width
                                height: 32
                                color: rowHover.containsMouse ? theme.accentSoft
                                     : index % 2 ? theme.cardAlt : "transparent"
                                Row {
                                    x: 8
                                    height: parent.height
                                    Text {
                                        width: processes.widths[0]; height: parent.height
                                        verticalAlignment: Text.AlignVCenter
                                        text: prow.what
                                        color: theme.subtle
                                        elide: Text.ElideRight
                                    }
                                    Text {
                                        width: processes.nameWidth(prow.width); height: parent.height
                                        verticalAlignment: Text.AlignVCenter
                                        text: prow.name + (prow.truncated ? "…" : "")
                                        color: prow.pickable ? theme.accent : theme.text
                                        elide: Text.ElideMiddle
                                        font.bold: prow.name === "UI thread"
                                    }
                                    Item {
                                        width: processes.widths[2]; height: parent.height
                                        Rectangle {
                                            id: dot
                                            visible: prow.hasStatus
                                            width: 8; height: 8; radius: 4
                                            anchors.verticalCenter: parent.verticalCenter
                                            color: window.stateColor(prow.status)
                                        }
                                        Text {
                                            anchors.left: dot.right
                                            anchors.leftMargin: 7
                                            anchors.verticalCenter: parent.verticalCenter
                                            text: prow.status
                                            color: theme.text
                                        }
                                    }
                                    Text {
                                        width: processes.widths[3]; height: parent.height
                                        verticalAlignment: Text.AlignVCenter
                                        horizontalAlignment: Text.AlignRight
                                        rightPadding: 12
                                        text: prow.work
                                        color: theme.text
                                        font.family: theme.mono
                                    }
                                    Text {
                                        width: processes.widths[4]; height: parent.height
                                        verticalAlignment: Text.AlignVCenter
                                        horizontalAlignment: Text.AlignRight
                                        rightPadding: 12
                                        text: prow.wakeups
                                        color: theme.text
                                        font.family: theme.mono
                                    }
                                    Text {
                                        width: processes.widths[5]; height: parent.height
                                        verticalAlignment: Text.AlignVCenter
                                        elide: Text.ElideRight
                                        color: theme.subtle
                                        text: prow.detail
                                    }
                                }
                                MouseArea {
                                    id: rowHover
                                    anchors.fill: parent
                                    hoverEnabled: prow.pickable
                                    enabled: prow.pickable
                                    Accessible.role: Accessible.ListItem
                                    Accessible.name: prow.name
                                    onClicked: {
                                        window.page = 3
                                        objects.load(true)
                                        objects.pick(prow.handle, prow.name)
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
                    // Row by row, as the processes are: a box whose
                    // figures are the same is left alone. There are few
                    // boxes, so all are set in one go.
                    ListModel { id: boxRows }
                    property var held: []

                    function show(list) {
                        var h = held
                        for (var i = 0; i < list.length; ++i) {
                            var row = flat(list[i])
                            if (i >= h.length) {
                                boxRows.append(row)
                                h.push(row)
                            } else if (processes.differs(h[i], row)) {
                                boxRows.set(i, row)
                                h[i] = row
                            }
                        }
                        if (h.length > list.length) {
                            boxRows.remove(list.length, h.length - list.length)
                            h.splice(list.length, h.length - list.length)
                        }
                        held = h
                    }

                    // A box as the view's model holds it: what the card
                    // shows, worked out once.
                    function flat(b) {
                        return {
                            name: b.name,
                            depth: b.depth,
                            capacity: b.capacity,
                            sent: String(b.sent),
                            drained: String(b.drained),
                            refused: b.refused,
                            replaced: String(b.replaced),
                            senders: b.senders + (b.waiting_senders > 0 ? " (" + b.waiting_senders + " waiting)" : ""),
                            oldest: b.oldest_us === undefined || b.oldest_us === null ? "—" : window.ms(b.oldest_us),
                            kinds: b.kinds.length === 0 ? "empty"
                                : b.kinds.map(function(k) { return k.kind + " ×" + k.count }).join(", ")
                        }
                    }

                    function load() {
                        window.ask("app_processes", { kind: "mailbox", peek: 5 }, "mailboxes")
                    }

                    RowLayout {
                        Text { text: "Mailboxes"; color: theme.text; font.pixelSize: 19; font.bold: true }
                        Text {
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
                        model: window.page === 2 ? boxRows : null
                        delegate: Item {
                            id: box
                            required property string name
                            required property int depth
                            required property int capacity
                            required property string sent
                            required property string drained
                            required property int refused
                            required property string replaced
                            required property string senders
                            required property string oldest
                            required property string kinds
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
                                        Text { text: box.name; color: theme.text; font.bold: true; font.pixelSize: 15 }
                                        Item { Layout.fillWidth: true }
                                        Text {
                                            text: box.depth + " / " + box.capacity
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
                                            width: parent.width * Math.min(1, box.depth / Math.max(1, box.capacity))
                                            color: box.depth >= box.capacity ? theme.bad
                                                 : box.depth > 0 ? theme.warn : theme.good
                                        }
                                    }
                                    GridLayout {
                                        columns: 4
                                        columnSpacing: 12
                                        rowSpacing: 2
                                        Text { text: "sent"; color: theme.subtle }
                                        Text { text: box.sent; color: theme.text; font.family: theme.mono }
                                        Text { text: "drained"; color: theme.subtle }
                                        Text { text: box.drained; color: theme.text; font.family: theme.mono }
                                        Text { text: "refused"; color: theme.subtle }
                                        Text {
                                            text: box.refused
                                            font.family: theme.mono
                                            color: box.refused > 0 ? theme.bad : theme.text
                                        }
                                        Text { text: "replaced"; color: theme.subtle }
                                        Text { text: box.replaced; color: theme.text; font.family: theme.mono }
                                        Text { text: "senders"; color: theme.subtle }
                                        Text { text: box.senders; color: theme.text; font.family: theme.mono }
                                        Text { text: "oldest"; color: theme.subtle }
                                        Text { text: box.oldest; color: theme.text; font.family: theme.mono }
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        elide: Text.ElideRight
                                        color: theme.subtle
                                        text: box.kinds
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
                    // The parts of a tree are put together in `tree` and
                    // shown once all have come. The view's rows are a
                    // ListModel brought to the rows wanted a piece at a
                    // time (`sync`): a row kept is not made again, and no
                    // frame does a whole tree of thousands.
                    property int treeCount: 0
                    property int shownCount: 0
                    property string filter: ""
                    onFilterChanged: show()
                    ListModel { id: treeRows }
                    // What `treeRows` holds and what it is to hold, where
                    // each wanted row is, how far down the tree the filter
                    // has looked for them, and how many rows from the top
                    // the two are already the same.
                    property var have: []
                    property var want: []
                    property var wantAt: ({})
                    property int scanned: 0
                    property int same: 0
                    // Rows to the piece, where the view shows them and
                    // elsewhere, and time to the frame that `sync` takes,
                    // well inside the dispatch's budget.
                    readonly property int near: 4
                    readonly property int piece: 200
                    readonly property int syncMs: 2
                    readonly property int rowHeight: 26
                    Timer {
                        id: syncing
                        interval: 1
                        repeat: true
                        onTriggered: objects.sync()
                    }
                    property bool truncated: false
                    // The tree comes in parts, each as much as one
                    // answer holds; a part of a tree asked before the
                    // last load is let go, and a part turned away is
                    // asked again in a while.
                    property int treeAsked: 0
                    property bool treeComing: false
                    property string partCursor: ""
                    // The application's object count when the tree was
                    // asked: objects made or let go at run time ask for
                    // it again, at most once a second.
                    property int treeObjects: -1
                    property real treeAskedAt: 0
                    readonly property int mostObjects: 3000
                    // An application with more objects than the tree
                    // holds is shown to a depth, and a row whose
                    // children were left out is opened by hand, as far
                    // down again: the rows shown are the ones looked
                    // for, not the first thousands in order.
                    readonly property int step: 2
                    property int treeDepth: 0
                    // The rows opened, in the order they were, opened
                    // again after a reload; the one being opened, how
                    // deep it is and where its next part goes.
                    property var opened: []
                    property var toOpen: []
                    property string opening: ""
                    property int openDepth: 0
                    property int spliceAt: 0
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

                    function load(tree) {
                        follow(tree)
                        if (picked !== "")
                            inspect(true)
                    }

                    // The tree is asked again when a reload or the
                    // application made or let go of objects, at most
                    // once a second and not while one comes: a count
                    // changed sooner is looked at again in a while.
                    function follow(tree) {
                        var count = overview.latest.objects
                        var counted = count !== undefined && count !== treeObjects
                        if (counted && !tree && (treeComing
                                || objects.tree.length > 0 && Date.now() - treeAskedAt < 1000)) {
                            if (!partAgain.running)
                                partAgain.start()
                            return
                        }
                        if (tree || (objects.tree.length === 0 && !treeComing) || counted) {
                            treeAsked += 1
                            treeObjects = count === undefined ? -1 : count
                            treeAskedAt = Date.now()
                            treeDepth = count !== undefined && count > mostObjects ? step : 0
                            partCursor = ""
                            opening = ""
                            toOpen = []
                            askPart()
                        } else if (!treeComing) {
                            goOn()
                        }
                    }

                    Timer {
                        id: partAgain
                        interval: 1100
                        onTriggered: objects.follow(false)
                    }

                    // The rest of the part being asked, or the next row
                    // to open.
                    function goOn() {
                        if (partCursor !== "" || opening !== "")
                            askPart()
                        else
                            openNext()
                    }

                    function askPart() {
                        treeComing = true
                        var room = mostObjects - tree.length
                        if (partCursor !== "")
                            window.ask("app_objects", { cursor: partCursor, max: room },
                                       "objects+:" + treeAsked)
                        else if (opening !== "")
                            // The row itself comes first, and is not
                            // counted against the room.
                            window.ask("app_objects", { root: opening, depth: step, max: room + 1 },
                                       "objects+:" + treeAsked)
                        else if (treeDepth > 0)
                            window.ask("app_objects", { max: mostObjects, depth: treeDepth },
                                       "objects:" + treeAsked)
                        else
                            window.ask("app_objects", { max: mostObjects }, "objects:" + treeAsked)
                    }

                    function turnedAway(asked) {
                        if (asked !== treeAsked)
                            return
                        treeComing = false
                        // A first part turned away is asked again too.
                        if (partCursor === "" && opening === "")
                            treeObjects = -1
                        // An application at rest asks nothing of the
                        // observer, so the part is asked again in a
                        // while, not at the next change.
                        if (!partAgain.running)
                            partAgain.start()
                    }

                    function indexOf(handle) {
                        for (var i = 0; i < tree.length; ++i)
                            if (tree[i].handle === handle)
                                return i
                        return -1
                    }

                    function isOpen(row) {
                        return row.hidden === undefined && opened.indexOf(row.handle) >= 0
                    }

                    function open(handle) {
                        if (opened.indexOf(handle) < 0)
                            opened = opened.concat([handle])
                        toOpen = toOpen.concat([handle])
                        if (!treeComing)
                            goOn()
                    }

                    // What is below a row is let go of, and so are the
                    // rows opened in it.
                    function close(handle) {
                        var i = indexOf(handle)
                        if (i < 0 || treeComing)
                            return
                        var row = tree[i]
                        var end = i + 1
                        while (end < tree.length && tree[end].depth > row.depth)
                            ++end
                        var gone = {}
                        for (var k = i + 1; k < end; ++k)
                            gone[tree[k].handle] = true
                        opened = opened.filter(function(h) { return h !== handle && !gone[h] })
                        row.hidden = row.children
                        tree = tree.slice(0, i).concat([row], tree.slice(end))
                        truncated = false
                        show()
                    }

                    function openNext() {
                        while (toOpen.length > 0) {
                            var handle = toOpen[0]
                            toOpen = toOpen.slice(1)
                            var i = indexOf(handle)
                            // Gone, or below a row no longer open.
                            if (i < 0) {
                                opened = opened.filter(function(h) { return h !== handle })
                                continue
                            }
                            if (tree[i].hidden === undefined)
                                continue
                            if (tree.length >= mostObjects) {
                                truncated = true
                                toOpen = []
                                break
                            }
                            opening = handle
                            askPart()
                            return
                        }
                        show()
                    }

                    function showTree(ok, s, r, asked, more) {
                        if (asked !== treeAsked)
                            return
                        treeComing = false
                        var going = partCursor !== ""
                        partCursor = ""
                        if (!ok) {
                            status.text = window.refusal(r)
                            // A row that cannot be opened is not opened
                            // again.
                            if (opening !== "") {
                                var lost = opening
                                opened = opened.filter(function(h) { return h !== lost })
                                opening = ""
                                goOn()
                            }
                            return
                        }
                        var rows = s.objects
                        if (!more) {
                            tree = rows
                            truncated = false
                            toOpen = opened.slice()
                        } else if (opening === "") {
                            tree = tree.concat(rows)
                        } else {
                            if (!going) {
                                // The row comes again, now without what
                                // it left out; what is below it goes
                                // after it.
                                var at = indexOf(opening)
                                if (at < 0) {
                                    opening = ""
                                    goOn()
                                    return
                                }
                                openDepth = tree[at].depth
                                rows[0].depth = openDepth
                                tree = tree.slice(0, at).concat(tree.slice(at + 1))
                                spliceAt = at
                            }
                            for (var k = 0; k < rows.length; ++k)
                                if (going || k > 0)
                                    rows[k].depth += openDepth
                            tree = tree.slice(0, spliceAt).concat(rows, tree.slice(spliceAt))
                            spliceAt += rows.length
                        }
                        if (s.next_cursor) {
                            if (tree.length < mostObjects) {
                                partCursor = s.next_cursor
                                askPart()
                                return
                            }
                            truncated = true
                        }
                        opening = ""
                        goOn()
                    }

                    // The view is to show the rows of the tree the filter
                    // lets through; `sync` looks for them.
                    function show() {
                        want = []
                        wantAt = {}
                        scanned = 0
                        same = 0
                        treeCount = tree.length
                        if (!syncing.running)
                            syncing.start()
                    }

                    // What the filter looks in, in lower case, worked out
                    // once a row.
                    function key(row) {
                        if (row.key === undefined)
                            row.key = (row.type + "\n" + (row.id || "") + "\n"
                                       + (row.object_name || "")).toLowerCase()
                        return row.key
                    }

                    // A row as the view's model holds it.
                    function flat(row) {
                        return {
                            handle: row.handle,
                            kind: row.type,
                            ident: row.id || "",
                            oname: row.object_name || "",
                            depth: row.depth || 0,
                            isItem: row.item === true,
                            dim: row.visible === false,
                            hidden: row.hidden === undefined ? -1 : row.hidden
                        }
                    }

                    function differs(held, row) {
                        return held.kind !== row.type || held.ident !== (row.id || "")
                            || held.oname !== (row.object_name || "")
                            || held.depth !== (row.depth || 0)
                            || held.isItem !== (row.item === true)
                            || held.dim !== (row.visible === false)
                            || held.hidden !== (row.hidden === undefined ? -1 : row.hidden)
                    }

                    // Brings the view's rows a piece nearer the rows
                    // wanted, from the top: a row that is the same stays,
                    // one changed is set, rows no longer wanted go and
                    // rows new come in, a piece at a time.
                    function sync() {
                        var h = have
                        var w = want
                        var at = wantAt
                        var began = Date.now()
                        var t = tree
                        var f = filter.toLowerCase()
                        var i = scanned
                        for (; i < t.length; ++i) {
                            if (i % 16 === 15 && Date.now() - began >= syncMs)
                                break
                            if (f !== "" && key(t[i]).indexOf(f) < 0)
                                continue
                            at[t[i].handle] = w.length
                            w.push(t[i])
                        }
                        scanned = i
                        if (i < t.length)
                            return
                        shownCount = w.length
                        // Rows that come or go where the view shows them
                        // have their delegates made then and there, so
                        // there they change a few at a time.
                        var shownTo = Math.ceil((treeView.contentY - treeView.originY
                                                 + treeView.height) / rowHeight)
                        var p = same
                        var done = false
                        while (Date.now() - began < syncMs) {
                            var most = p <= shownTo ? near : piece
                            if (p >= w.length) {
                                if (h.length > p) {
                                    treeRows.remove(p, h.length - p)
                                    h.splice(p, h.length - p)
                                }
                                done = true
                                break
                            }
                            var j = p < h.length ? at[h[p].handle] : w.length
                            if (j === p) {
                                if (differs(h[p], w[p])) {
                                    h[p] = flat(w[p])
                                    treeRows.set(p, h[p])
                                }
                                ++p
                            } else if (j === undefined || j < p) {
                                // Gone, or already come in above.
                                var end = p + 1
                                while (end < h.length && end - p < most) {
                                    var k = at[h[end].handle]
                                    if (k !== undefined && k >= p)
                                        break
                                    ++end
                                }
                                treeRows.remove(p, end - p)
                                h.splice(p, end - p)
                            } else {
                                var rows = []
                                var stop = Math.min(j, p + most)
                                for (var n = p; n < stop; ++n)
                                    rows.push(flat(w[n]))
                                treeRows.insert(p, rows)
                                Array.prototype.splice.apply(h, [p, 0].concat(rows))
                                p = stop
                            }
                        }
                        same = p
                        have = h
                        if (done)
                            syncing.stop()
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
                            head = window.refusal(r)
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
                        pictureNote = ok ? "" : window.refusal(r)
                        if (!ok) {
                            picture = ""
                            return
                        }
                        for (var i = 0; i < r.content.length; ++i)
                            if (r.content[i].type === "image")
                                picture = "data:" + r.content[i].mimeType + ";base64," + r.content[i].data
                    }

                    // A value as a field shows it.
                    function valueText(v) {
                        return window.shown(v)
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
                            // A field is not named by its placeholder.
                            Accessible.name: "Filter objects"
                            onTextChanged: objects.filter = text
                        }
                        Text {
                            x: 12
                            anchors.top: treeFilter.bottom
                            anchors.topMargin: 6
                            color: theme.subtle
                            font.pixelSize: 11
                            text: objects.shownCount + " of " + objects.treeCount + " objects"
                                  + (objects.treeComing || objects.partCursor !== "" ? ", more coming"
                                     : objects.truncated ? ", the most shown"
                                     : objects.treeDepth > 0 ? " (" + objects.treeObjects + " in all; + opens a row)"
                                     : "")
                        }
                        ListView {
                            id: treeView
                            anchors.fill: parent
                            anchors.topMargin: treeFilter.height + 34
                            clip: true
                            model: window.page === 3 ? treeRows : null
                            // Where it was scrolled to, kept while the
                            // page is out of sight and its rows let go.
                            property real keptY: 0
                            onContentYChanged: if (model !== null) keptY = contentY
                            onModelChanged: if (model !== null) Qt.callLater(function() { treeView.contentY = treeView.keptY })
                            ScrollBar.vertical: ScrollBar {}
                            delegate: Rectangle {
                                id: node
                                required property string handle
                                required property string kind
                                required property string ident
                                required property string oname
                                required property int depth
                                required property bool isItem
                                required property bool dim
                                required property int hidden
                                width: ListView.view.width
                                height: objects.rowHeight
                                color: objects.picked === node.handle ? theme.accentSoft
                                     : nodeHover.containsMouse ? Qt.rgba(theme.text.r, theme.text.g, theme.text.b, 0.05)
                                     : "transparent"
                                Row {
                                    x: 10 + (objects.filter === "" ? node.depth * 14 : 0)
                                    height: parent.height
                                    spacing: 6
                                    Rectangle {
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: 6; height: 6; radius: node.isItem ? 1 : 3
                                        color: !node.isItem ? theme.violet
                                             : node.dim ? theme.subtle : theme.accent
                                    }
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: node.kind
                                        color: node.dim ? theme.subtle : theme.text
                                        font.bold: objects.picked === node.handle
                                    }
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: text !== ""
                                        text: node.ident ? "#" + node.ident
                                            : node.oname ? "\"" + node.oname + "\"" : ""
                                        color: theme.accent
                                    }
                                    // Without an id or a name, the handle tells
                                    // one Timer from the next; so it does
                                    // for the rows of a filter, which all
                                    // may have the id of their component.
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: objects.filter !== "" || !node.ident && !node.oname
                                        text: node.handle
                                        color: theme.subtle
                                        font.pixelSize: 11
                                    }
                                    // What a row left out opens it; an
                                    // opened row closes.
                                    Text {
                                        id: disclosure
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: text !== ""
                                        text: node.hidden >= 0 ? "+" + node.hidden
                                            : objects.opened.indexOf(node.handle) >= 0 ? "−" : ""
                                        color: discloseHover.containsMouse ? theme.accent : theme.subtle
                                        font.bold: discloseHover.containsMouse
                                    }
                                }
                                MouseArea {
                                    id: nodeHover
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    Accessible.role: Accessible.ListItem
                                    Accessible.name: node.kind + (node.ident ? "#" + node.ident : "")
                                    onClicked: objects.pick(node.handle,
                                                            node.kind + (node.ident ? "#" + node.ident : ""))
                                }
                                MouseArea {
                                    id: discloseHover
                                    visible: disclosure.visible
                                    x: disclosure.parent.x + disclosure.x - 6
                                    width: disclosure.width + 12
                                    height: parent.height
                                    hoverEnabled: true
                                    Accessible.role: Accessible.Button
                                    Accessible.name: node.hidden >= 0 ? "Open" : "Close"
                                    onClicked: {
                                        if (node.hidden >= 0)
                                            objects.open(node.handle)
                                        else
                                            objects.close(node.handle)
                                    }
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
                                Text {
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
                            Text {
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
                                model: window.page === 3 ? objects.propRows : 0
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
                                            // The check below speaks for a true or false value.
                                            Accessible.ignored: prop.boolean
                                            Accessible.role: Accessible.Button
                                            Accessible.name: prop.modelData.name || ""
                                            Accessible.description: objects.valueText(prop.modelData.value)
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
                                                // Named as the value it takes
                                                // the place of, so that what
                                                // pressed that value finds it.
                                                Accessible.name: prop.modelData.name || ""
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
                                            // That the binding is kept below an
                                            // edit is told on hover: in the chip
                                            // it left the value too narrow to
                                            // click.
                                            readonly property string kept: prop.edited && prop.modelData.binding_kept
                                                ? "The binding is kept below the edit; Revert brings it back" : ""
                                            Accessible.role: Accessible.StaticText
                                            Accessible.name: chipText.text
                                            Accessible.description: kept
                                            ToolTip.text: kept
                                            ToolTip.visible: kept !== "" && chipArea.containsMouse
                                            ToolTip.delay: 400
                                            Text {
                                                id: chipText
                                                anchors.centerIn: parent
                                                text: prop.edited ? (prop.modelData.provenance === "bound" ? "rebound" : "edited")
                                                    : prop.fixed ? prop.modelData.provenance + " · read-only"
                                                    : prop.modelData.provenance
                                                color: parent.tone
                                                font.pixelSize: 11
                                            }
                                            MouseArea {
                                                id: chipArea
                                                anchors.fill: parent
                                                enabled: parent.kept !== ""
                                                hoverEnabled: enabled
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
                                                Accessible.role: Accessible.Button
                                                Accessible.name: "Revert " + (prop.modelData.name || "")
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
                            Accessible.name: "The picked object as drawn"
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
                            Accessible.name: "The picked object as drawn"
                            anchors.centerIn: parent
                            z: frame.front === 1 ? 1 : 0
                            opacity: frame.front === 1 ? 1 : 0
                            width: Math.min(implicitWidth, frame.width - 24)
                            height: Math.min(implicitHeight, frame.height - 24)
                            fillMode: Image.PreserveAspectFit
                            onStatusChanged: frame.loaded(shotB)
                        }
                        Text {
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
                        // A QML warning starts with the full URL of where it
                        // happened, which pushed what happened out of the row;
                        // the last column already says where.
                        var at = e.file ? /^\S+:\d+(:\d+)?: /.exec(e.message) : null
                        shownLines.append({
                            level: e.level,
                            category: e.category || "",
                            message: at ? e.message.slice(at[0].length) : e.message,
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
                        Text { text: "Logs"; color: theme.text; font.pixelSize: 19; font.bold: true }
                        Text {
                            text: shownLines.count + " of " + logs.entries.length + " lines"
                            color: theme.subtle
                        }
                        Item { Layout.fillWidth: true }
                        TextField {
                            Layout.preferredWidth: 260
                            placeholderText: "Filter text or category"
                            Accessible.name: "Filter logs"
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
                            model: window.page === 4 ? shownLines : null
                            onModelChanged: if (model !== null) Qt.callLater(logView.positionViewAtEnd)
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
                                    Text {
                                        width: 64
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: entry.level
                                        color: logs.tone(entry.level)
                                        font.bold: entry.level === "warning" || entry.level === "critical"
                                        font.pixelSize: 12
                                    }
                                    Text {
                                        width: 110
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: entry.category
                                        color: theme.subtle
                                        elide: Text.ElideRight
                                        font.pixelSize: 12
                                    }
                                    Text {
                                        width: entry.width - 64 - 110 - 160 - 50
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: entry.message
                                        color: theme.text
                                        elide: Text.ElideRight
                                        font.family: theme.mono
                                        font.pixelSize: 12
                                    }
                                    Text {
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
                        Text {
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
                            a.holder = window.holder
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
                        // A string is quoted, so that "" and a number
                        // held as text read as what they are.
                        function said(v) {
                            return typeof v === "string" ? JSON.stringify(v) : window.shown(v)
                        }
                        var what = e.kind === "change" ? (e.key ? e.key + " (" + e.value + ")" : said(e.value))
                                 : e.kind === "signal" ? "(" + e.args.map(said).join(", ") + ")"
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
                        Text { text: "Trace"; color: theme.text; font.pixelSize: 19; font.bold: true }
                        Text {
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
                        Text {
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
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: chip.modelData.name
                                        color: theme.accent
                                        font.pixelSize: 12
                                    }
                                    Text {
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
                            model: window.page === 5 ? traceLines : null
                            onModelChanged: if (model !== null) Qt.callLater(traceView.positionViewAtEnd)
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
                                    Text {
                                        width: 80
                                        anchors.verticalCenter: parent.verticalCenter
                                        horizontalAlignment: Text.AlignRight
                                        text: traceRow.at
                                        color: theme.subtle
                                        font.family: theme.mono
                                        font.pixelSize: 12
                                    }
                                    Text {
                                        width: 180
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: traceRow.object
                                        color: theme.text
                                        elide: Text.ElideRight
                                        font.pixelSize: 12
                                    }
                                    Text {
                                        width: 70
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: traceRow.kind
                                        color: trace.tone(traceRow.kind)
                                        font.pixelSize: 12
                                    }
                                    Text {
                                        width: 150
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: traceRow.name
                                        color: theme.text
                                        font.bold: true
                                        elide: Text.ElideRight
                                        font.pixelSize: 12
                                    }
                                    Text {
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
                        Text {
                            anchors.centerIn: parent
                            visible: traceLines.count === 0
                            text: "Nothing written down yet"
                            color: theme.subtle
                        }
                    }
                }
            }

            // What the last refusal or failure said.
            Text {
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
