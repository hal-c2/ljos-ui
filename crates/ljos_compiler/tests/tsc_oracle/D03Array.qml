// expect: accept
// Decoder: `Array.isArray` of what is unknown, each element narrowed before it is used.
import Ljos.Ui
Item {
    property real shown: 0
    function take(v: unknown): void {
        if (!Array.isArray(v)) { return }
        for (const x of v) { if (typeof x === "number") { shown = shown + x } }
    }
}
