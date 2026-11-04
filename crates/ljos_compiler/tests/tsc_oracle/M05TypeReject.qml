// expect: reject-type 6 TS2724
// A property of a type the module does not export.
import Ljos.Ui
import "M05Type.ts" as Logic
Item {
    property Logic.Rows kept
}
