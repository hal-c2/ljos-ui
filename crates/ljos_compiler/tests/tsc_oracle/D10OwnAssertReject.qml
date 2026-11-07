// expect: reject-type 7 TS2775
// Decoder: an assertion function a component declares is reached through the component, which has no written type.
import Ljos.Ui
Item {
    property real shown: 0
    function check(a: real, v: unknown): asserts v is number { if (typeof v !== "number") { throw new Error("no") } }
    function take(v: unknown): void { check(1, v); shown = v }
}
