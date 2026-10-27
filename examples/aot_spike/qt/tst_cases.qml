import QtQuick
import QtTest
import "../qml"

// The spike's cases as a script drives them, for a runtime only run as a
// black box (qml6's qmltestrunner): each case's step (its inputs, then its
// trigger) timed over many rounds, and its inputs alone, as src/lib.rs's
// `prepare` and `trigger` give them. A binding is read after its inputs
// change; a handler is started by emitting its signal.
TestCase {
    name: "Cases"
    property int rounds: 200000

    World { id: w }

    readonly property var statuses: ["todo", "doing", "blocked", "done", "dropped", "archived"]
    readonly property var keys: [[0x31, 0x04000000], [0x01000015, 0], [0x01000003, 0], [0x41, 0]]

    function inputs(c, k) {
        switch (c) {
        case "c1": w.total = [10, 0, 7][k % 3]; w.done = k % 11; break;
        case "c2": w.stopMode = k % 2 === 0; break;
        case "c4": w.status = statuses[k % 6]; break;
        case "c7": w.depth = k % 3; w.children[2].text = k % 2 === 0 ? "" : "x"; break;
        }
    }

    function trigger(c, k) {
        switch (c) {
        case "c1": return w.progress;
        case "c2": w.clicked(); return 0;
        case "c3": w.children[1].submitted(k % 2 === 0 ? "foreground" : "background"); return 0;
        case "c4": return w.statusText;
        case "c7": {
            const p = keys[k % 4];
            w.keyPressed(k % 4 === 0 ? p[0] + k % 9 : p[0], p[1]);
            return 0;
        }
        }
        return 0;
    }

    function ns(ms) {
        return Math.round(ms * 1e6 / rounds);
    }

    function test_cases() {
        let sink = 0;
        for (const c of ["c1", "c2", "c3", "c4", "c7"]) {
            for (let k = 0; k < 2000; ++k) {
                inputs(c, k);
                trigger(c, k);
            }
            let t0 = Date.now();
            for (let k = 0; k < rounds; ++k)
                inputs(c, k);
            const alone = Date.now() - t0;
            t0 = Date.now();
            for (let k = 0; k < rounds; ++k) {
                inputs(c, k);
                sink += trigger(c, k) ? 1 : 0;
            }
            const step = Date.now() - t0;
            console.log("case", c, "step_ns", ns(step), "inputs_ns", ns(alone));
        }
        verify(sink >= 0);
    }
}
