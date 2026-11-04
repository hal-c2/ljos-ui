// expect: accept
// Decoder: a script's own function may be a guard.
import Ljos.Ui
Item {
    property real shown: 0
    function isBig(v: unknown): v is number { return typeof v === "number" && v > 10 }
    function take(v: unknown): void { shown = isBig(v) ? v : 0 }
}
