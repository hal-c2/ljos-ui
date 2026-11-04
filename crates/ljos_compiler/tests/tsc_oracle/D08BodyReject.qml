// expect: reject-type 6 TS2322
// Decoder: a guard's body gives a boolean, and a number is not one.
import Ljos.Ui
Item {
    property real shown: 0
    function isTag(v: unknown): v is string { return 1 }
}
