// expect: accept
// Shape: a file writes a type of Ljos.Ui and sets a member of it in a script.
import Ljos.Ui
Item {
    property Item i: Item {}
    signal picked(item: Rectangle)
    function f(): void { i.width = 3 }
    onPicked: item => { item.radius = 2 }
}
