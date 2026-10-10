import QtQuick

// Generated scaffolding for the Ljós BDD Qt oracle. A plain switch over ops;
// the only dynamic code is the `bind` expressions, which are written by the
// scenario author and compiled as ordinary functions below.
Item {
    id: driver
    width: 400
    height: 400

    property var program: /*PROGRAM*/[]
    property string dir: /*DIR*/""
    property string grabPath: /*GRAB*/""
    property var roots: []
    property var out: []
    property int pc: 0
    // Input steps: `waiting` until the pointer move that ends a step's events
    // arrives (rfb.rs), counted in `acks`.
    property bool waiting: false
    property int acks: 0
    property bool wantActive: true
    property var other: null

/*BINDERS*/
    function ser(v) {
        if (v === undefined || v === null) return null
        var t = typeof v
        if (t === "number") return isFinite(v) ? v : String(v)
        if (t === "string" || t === "boolean") return v
        // Value types (color, point, ...) have no stable JSON form; use their text.
        if (v.r !== undefined && v.g !== undefined && v.b !== undefined) return String(v)
        try { return JSON.parse(JSON.stringify(v)) } catch (e) { return String(v) }
    }

    function kids(o) {
        var all = []
        var lists = [o.data, o.resources]
        for (var l = 0; l < lists.length; ++l) {
            var ds = lists[l]
            if (ds) for (var i = 0; i < ds.length; ++i) all.push(ds[i])
        }
        return all
    }

    function deep(o, name) {
        if (o.objectName === name) return o
        var ks = kids(o)
        for (var i = 0; i < ks.length; ++i) {
            if (!ks[i]) continue
            var f = deep(ks[i], name)
            if (f) return f
        }
        return null
    }

    // "list/row" finds "list" anywhere, then "row" anywhere below it.
    function find(path) {
        var parts = path.split("/")
        var cur = null
        for (var p = 0; p < parts.length; ++p) {
            var next = null
            if (cur === null) {
                for (var r = 0; r < roots.length && !next; ++r) next = deep(roots[r], parts[p])
            } else {
                next = deep(cur, parts[p])
            }
            if (!next) throw new Error("no object named \"" + parts[p] + "\"")
            cur = next
        }
        return cur
    }

    // "anchors.margins" resolves the owner of the last segment.
    function owner(o, prop) {
        var parts = prop.split(".")
        for (var i = 0; i < parts.length - 1; ++i) o = o[parts[i]]
        return { o: o, p: parts[parts.length - 1] }
    }

    function exec(op) {
        switch (op.op) {
        case "load": {
            var c = Qt.createComponent(dir + op.file)
            if (c.status !== Component.Ready) throw new Error("cannot load " + op.file + ": " + c.errorString())
            var obj = c.createObject(driver)
            if (!obj) throw new Error("cannot create " + op.file + ": " + c.errorString())
            roots.push(obj)
            return null
        }
        case "set": { var s = owner(find(op.obj), op.prop); s.o[s.p] = op.value; return null }
        case "bind": { var b = owner(find(op.obj), op.prop); b.o[b.p] = driver["bind" + op.binder](find(op.obj)); return null }
        case "read": { var r = owner(find(op.obj), op.prop); return ser(r.o[r.p]) }
        case "call": { var t = find(op.obj); return ser(t[op.method].apply(t, op.args)) }
        case "exists": { try { find(op.obj); return true } catch (e) { return false } }
        case "language": { Qt.uiLanguage = op.value; return null }
        case "nop": return null
        default: throw new Error("unknown op " + op.op)
        }
    }

    function finish() {
        console.log("@@ljos@@" + JSON.stringify(out))
        Qt.quit()
    }

    function run() {
        while (pc < program.length) {
            var op = program[pc++]
            if (op.op === "wait") { pause.interval = op.ms; pause.restart(); out.push({}); return }
            if (op.op === "grab") {
                out.push({})
                driver.grabToImage(function(res) { res.saveToFile(grabPath); run() })
                return
            }
            if (op.op === "key" || op.op === "pointer") {
                out.push({})
                waiting = true
                console.log("@@input@@" + (pc - 1))
                return
            }
            if (op.op === "active") { out.push({}); activate(op.value); return }
            try { out.push({ value: exec(op) }) } catch (e) { out.push({ error: String(e.message || e) }) }
        }
        finish()
    }

    Timer { id: pause; repeat: false; onTriggered: driver.run() }

    // Another window takes the keyboard, as when the user switches windows.
    function activate(on) {
        if (on) {
            driver.Window.window.requestActivate()
        } else {
            if (!other) other = otherWindow.createObject(driver)
            other.visible = true
            other.requestActivate()
        }
        wantActive = on
        settle.restart()
    }
    Component { id: otherWindow; Window { x: 0; y: 0; width: 1; height: 1; flags: Qt.FramelessWindowHint } }
    // An inactive window has no active focus item; an active one always has
    // one (its content item at least).
    Timer {
        id: settle; interval: 5; repeat: true
        onTriggered: if ((driver.Window.activeFocusItem !== null) === driver.wantActive) { stop(); driver.run() }
    }
/*INPUT*/

    // Focus follows the active window, and a window becomes active a moment
    // after it is shown; the program starts then. The timer is for a platform
    // that never activates a window: late is better than never.
    property bool started: false
    function start() {
        if (started) return
        started = true
        run()
    }
    Window.onActiveFocusItemChanged: if (Window.activeFocusItem) Qt.callLater(start)
    Timer { interval: 1000; running: true; onTriggered: driver.start() }
    Component.onCompleted: if (Window.activeFocusItem) Qt.callLater(start)
}
