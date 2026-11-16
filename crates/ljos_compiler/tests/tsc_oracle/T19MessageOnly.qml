// expect: accept
// Shape: a message is given to t by its text and context, without an id.
import Ljos.Ui; import Ljos.Core
Item {
    property string open: t({ message: "Open", context: "menu" })
    property string home: t({ id: "nav.home" })
}
