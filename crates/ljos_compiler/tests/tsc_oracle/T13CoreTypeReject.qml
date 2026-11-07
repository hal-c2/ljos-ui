// expect: reject-type 6 TS2304
// Shape: a file of Ljos.Ui alone reads an enum of a type of Ljos.Core.
import Ljos.Ui
Item {
    property int status: 0
    function check() { status = Component.Ready }
}
