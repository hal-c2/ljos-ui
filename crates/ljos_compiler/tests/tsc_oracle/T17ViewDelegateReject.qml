// expect: reject-type 7 TS2322
// Shape: a file of Ljos.Ui alone gives a view's delegate to a number.
import Ljos.Ui
Item {
    property int count: 0
    ListView { id: list; delegate: Item { } }
    function count_it() { count = list.delegate }
}
