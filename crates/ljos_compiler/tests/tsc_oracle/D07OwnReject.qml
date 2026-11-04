// expect: reject-type 6 TS2677
// Decoder: a guard may not narrow a parameter to what it cannot be.
import Ljos.Ui
Item {
    property real shown: 0
    function isCount(v: string): v is number { return false }
}
