// expect: reject-type 6 TS2322
// Shape: a signal's parameter of a type of Ljos.Ui is given a string in a handler.
import Ljos.Ui
Item {
    signal picked(item: Rectangle)
    onPicked: item => { item.radius = "round" }
}
