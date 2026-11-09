// expect: reject-type 7 TS2322
// Shape: what a shadowing property holds is what the file declares.
import Ljos.Ui
Item {
    property string top: "first"
    property int size: 0
    function grow() { size = top }
}
