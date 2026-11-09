// expect: accept
// Shape: a property shadows the member of that name its type has, whatever
// that one holds (beside Qt's table `Item.top` is an anchor line).
import Ljos.Ui
Item {
    property string top: "first"
    property int size: top.length
}
