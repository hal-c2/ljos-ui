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
    // The step of a held pointer the driver waits to see (qt.rs).
    property var expect: null
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
        var lists = [o.data, o.resources, o.contentData]
        for (var l = 0; l < lists.length; ++l) {
            var ds = lists[l]
            if (ds) for (var i = 0; i < ds.length; ++i) all.push(ds[i])
        }
        // A menu behind a menu item and the popup of a combo box are in
        // none of the lists.
        var more = [o.subMenu, o.popup]
        for (var m = 0; m < more.length; ++m)
            if (more[m] && typeof more[m] === "object") all.push(more[m])
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
        case "copy": { var cc = clipper(); cc.text = op.value; cc.selectAll(); cc.copy(); return null }
        case "paste": { var cp = clipper(); cp.text = ""; cp.paste(); return cp.text }
        case "copyHtml": { var ch = richClipHelper.createObject(driver, { text: op.value }); ch.selectAll(); ch.copy(); ch.destroy(); return null }
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
            if (["key", "pointer", "type", "double", "drag"].indexOf(op.op) >= 0) {
                out.push({})
                expect = op.step ? op : null
                waiting = true
                console.log("@@input@@" + (pc - 1))
                return
            }
            if (op.op === "active") { out.push({}); activate(op.value); return }
            if (op.op === "touches") {
                try { touches(op.frames); out.push({}) } catch (e) { out.push({ error: String(e.message || e) }) }
                continue
            }
            try { out.push({ value: exec(op) }) } catch (e) { out.push({ error: String(e.message || e) }) }
        }
        finish()
    }

    Timer { id: pause; repeat: false; onTriggered: driver.run() }

    // Fingers, frame by frame, 16 ms apart, through QtTest's touch events:
    // a finger that stays where it was is told as stationary, which a
    // PinchArea needs. Made when used, so that only these programs import
    // QtTest.
    property var tester: null
    function touches(frames) {
        if (!tester) tester = Qt.createQmlObject("import QtTest; TestCase { when: false; running: false }", driver)
        var c = driver.Window.window.contentItem
        var seq = tester.touchEvent(c)
        var down = {}
        frames.push([])
        for (var f = 0; f < frames.length; f++) {
            var now = {}
            for (var i = 0; i < frames[f].length; i++) now[frames[f][i][0]] = frames[f][i]
            var any = false
            for (var id in down) if (!(id in now)) { seq.release(Number(id), c, down[id][1], down[id][2]); any = true }
            for (var id2 in now) {
                var p = now[id2], was = down[id2]
                if (!was) seq.press(p[0], c, p[1], p[2])
                else if (was[1] !== p[1] || was[2] !== p[2]) seq.move(p[0], c, p[1], p[2])
                else seq.stationary(p[0])
                any = true
            }
            if (any) { seq.commit(); tester.wait(16) }
            down = now
        }
    }

    // The clipboard has no QML API of its own; an editor that is never
    // shown copies into it and pastes out of it.
    property var clip: null
    function clipper() {
        if (!clip) clip = clipHelper.createObject(driver)
        return clip
    }
    Component { id: clipHelper; TextEdit { visible: false; textFormat: TextEdit.PlainText } }
    // A rich text editor copies HTML, and the characters as plain text.
    Component { id: richClipHelper; TextEdit { visible: false; textFormat: TextEdit.RichText } }

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
/*HELD*/

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
