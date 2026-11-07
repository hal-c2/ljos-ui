// expect: reject-type 6 TS2322
// Shape: a file writes a type of Ljos.Ui and gives a member of it a string.
import Ljos.Ui
Item {
    property Item i: Item {}
    function f(): void { i.width = "wide" }
}
