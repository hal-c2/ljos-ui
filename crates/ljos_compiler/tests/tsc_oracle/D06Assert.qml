// expect: accept
// Decoder: an assertion function narrows what follows its call.
import Ljos.Ui
import "D04Guard.ts" as Logic
Item {
    property real shown: 0
    function take(v: unknown): void { Logic.row(v); shown = v.count }
}
