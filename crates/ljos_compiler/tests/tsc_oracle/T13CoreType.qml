// expect: accept
// Shape: a file that imports Ljos.Core reads an enum of one of its types.
import Ljos.Ui; import Ljos.Core
Item {
    property int status: 0
    function check() { status = Component.Ready }
}
