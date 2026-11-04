// expect: accept
// Shape: a status copied into a variable that a loop may move on; the copy keeps the status's type.
import Ljos.Ui
Item {
    property string shown: ""
    function onlyKnown(s: "todo" | "doing" | "done"): void { shown = s }
    function step(given: "todo" | "doing" | "done"): void {
        let status = given
        onlyKnown(status)
    }
}
