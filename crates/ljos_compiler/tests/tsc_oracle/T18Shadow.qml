// expect: accept
// Shape: a property shadows the member of that name its type has, whatever
// that one holds (beside Qt's table `Item.clip` is a bool).
import Ljos.Ui
Item {
    property string clip: "first"
    property int size: clip.length
}
