// expect: accept
// Shape: a file of Ljos.Ui alone hands one view's delegate to another.
import Ljos.Ui
Item {
    ListView { id: list; delegate: Item { } }
    Repeater { id: again }
    function share() { again.delegate = list.delegate }
}
