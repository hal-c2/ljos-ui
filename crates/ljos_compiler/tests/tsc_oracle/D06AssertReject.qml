// expect: reject-type 7 TS2322
// Decoder: what an assertion narrows to is typed: a `Row`'s tag is no number.
import Ljos.Ui
import "D04Guard.ts" as Logic
Item {
    property real shown: 0
    function take(v: unknown): void { Logic.row(v); shown = v.tag }
}
