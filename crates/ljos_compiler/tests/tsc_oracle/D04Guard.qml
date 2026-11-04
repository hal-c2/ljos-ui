// expect: accept
// Decoder: a type guard narrows in `if`, `&&`, `?:` and after an early return.
import Ljos.Ui
import "D04Guard.ts" as Logic
Item {
    property unknown given
    property real shown: 0
    property string label: ""
    function viaIf(): void { if (Logic.isRow(given)) { shown = given.count } }
    function viaAnd(): void { label = Logic.isRow(given) && given.tag === "a" ? "a" : "other" }
    function viaReturn(v: unknown): number { if (!Logic.isRow(v)) { return 0 } return v.count }
}
