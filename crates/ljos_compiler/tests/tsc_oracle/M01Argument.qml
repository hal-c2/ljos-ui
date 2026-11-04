// expect: accept
// A function a module exports, called with what its parameter takes.
import Ljos.Ui
import "M01Argument.ts" as Logic
Item {
    property real shown: 0
    function choose(): void { shown = Logic.only("a") }
}
