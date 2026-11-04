// expect: reject-type 7 TS2345
// A function a module exports, called with a number for its `Tag`.
import Ljos.Ui
import "M01Argument.ts" as Logic
Item {
    property real shown: 0
    function choose(): void { shown = Logic.only(2) }
}
