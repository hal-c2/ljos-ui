// expect: reject-type 7 TS2775
// Decoder: an assertion through a const arrow with no written type narrows nothing; the arrow's own return type is not the const's.
import Ljos.Ui
Item {
    property string shown: ""
    function take(v: unknown): void {
        const check = (x: unknown): asserts x is string => { if (typeof x !== "string") { throw new Error("no") } }; check(v); shown = v
    }
}
