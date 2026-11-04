// expect: reject-type 11 TS2345
// Shape: a task's status, two of whose cases share one body, given where only the second goes.
import Ljos.Ui
Item {
    property string shown: ""
    function onlyDoing(s: "doing"): void { shown = s }
    function step(status: "todo" | "doing" | "done"): void {
        switch (status) {
        case "todo":
        case "doing":
            onlyDoing(status)
            break
        default:
            break
        }
    }
}
