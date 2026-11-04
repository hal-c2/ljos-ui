// expect: accept
// Shape: a task's status, two of whose cases share one body.
import Ljos.Ui
Item {
    property string shown: ""
    function onlyTodo(s: "todo"): void { shown = s }
    function step(status: "todo" | "doing" | "done"): void {
        switch (status) {
        case "todo":
        case "doing":
            if (status === "todo") onlyTodo(status)
            break
        default:
            break
        }
    }
}
