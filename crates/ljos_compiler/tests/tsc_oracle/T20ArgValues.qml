// expect: accept
// Shape: arg takes several values in one call, each a string, number or boolean.
import Ljos.Ui
Item {
    property string page: "Settings"
    property int count: 3
    property string title: "%1 — %2 (%3, %4)".arg(page, "app", count, true)
}
