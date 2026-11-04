// expect: reject-type 13 TS2345
// Shape: a case finishes a task and falls into the next, which takes only a task not yet done.
import Ljos.Ui
Item {
    property string shown: ""
    function onlyNotDone(s: "todo" | "doing"): void { shown = s }
    function step(given: "todo" | "doing" | "done"): void {
        let status: "todo" | "doing" | "done" = given
        switch (status) {
        case "todo":
            status = "done"
        case "doing":
            onlyNotDone(status)
            break
        default:
            break
        }
    }
}
