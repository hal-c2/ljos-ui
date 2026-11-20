// expect: reject-type 6 TS2345
// Shape: a value after the first is still a string, number or boolean.
import Ljos.Ui
Item {
    property string page: "Settings"
    property string title: "%1 — %2".arg(page, {})
}
