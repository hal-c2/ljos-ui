// expect: accept
// Decoder: an assertion through a const whose type is written narrows what it is given.
import Ljos.Ui
Item {
    property string shown: ""
    function take(v: unknown): void {
        const check: (x: unknown) => asserts x is string = (x) => { if (typeof x !== "string") { throw new Error("no") } }; check(v); shown = v
    }
}
