// expect: accept
// Shape: a case moves a task on and falls into the next, which sees what the first made of it.
import Ljos.Ui
Item {
    property string shown: ""
    function onlyDoing(s: "doing"): void { shown = s }
    function step(given: "todo" | "doing" | "done"): void {
        let status: "todo" | "doing" | "done" = given
        switch (status) {
        case "todo":
            status = "doing"
        case "doing":
            onlyDoing(status)
            break
        default:
            break
        }
    }
}
