// expect: reject-type 6 TS2551
// A value the module does not export.
import Ljos.Ui
import "M04Export.ts" as Logic
Item {
    property string title: Logic.lable
}
