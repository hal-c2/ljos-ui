import QtQuick

// The boundary of the dynamic tiers: what one more tracked read, write or
// emit costs. r33 has 32 reads of a where r1 has one; kw's handler writes
// once where kn's does nothing; ke's emits once.
Item {
    property real a: 1
    property real k: 0
    property real sink: 0
    signal ping(string s)

    property real r1: k + a
    property real r33: k + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a + a

    property real kw: 0
    onKwChanged: sink = kw
    property real kn: 0
    onKnChanged: {}
    property real ke: 0
    onKeChanged: ping("x")

    function readLoop(n) {
        let s = k;
        for (let i = 0; i < n; ++i)
            s += a;
        return s;
    }

    function writeLoop(n) {
        for (let i = 0; i < n; ++i)
            sink = i;
        return n;
    }

    function emitLoop(n) {
        for (let i = 0; i < n; ++i)
            ping("x");
        return n;
    }
}
