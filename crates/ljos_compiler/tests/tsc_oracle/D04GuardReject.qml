// expect: reject-type 8 TS18046
// Decoder: a guard's branch that falls through narrows nothing after it.
import Ljos.Ui
import "D04Guard.ts" as Logic
Item {
    property unknown given
    property real shown: 0
    function missed(): void { if (!Logic.isRow(given)) { shown = 1 } shown = given.count }
}
