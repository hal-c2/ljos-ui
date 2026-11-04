// expect: accept
// Decoder: a member found with `in` and checked with `typeof`, after an early return.
import Ljos.Ui
Item {
    property real shown: 0
    function take(v: unknown): void {
        if (typeof v !== "object" || v === null || !("count" in v)) { return }
        if (typeof v.count === "number") { shown = v.count }
    }
}
