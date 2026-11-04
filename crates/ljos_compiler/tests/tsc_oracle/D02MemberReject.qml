// expect: reject-type 8 TS2322
// Decoder: a member found with `in` is unknown until it is checked.
import Ljos.Ui
Item {
    property real shown: 0
    function take(v: unknown): void {
        if (typeof v !== "object" || v === null || !("count" in v)) { return }
        shown = v.count
    }
}
