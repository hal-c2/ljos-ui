// expect: accept
// Shape: a value that is a status or a written negative count; only the count widens, so the status stays one.
import Ljos.Ui
Item {
    property string shown: ""
    function onlyKnown(s: "todo" | "doing" | "done"): void { shown = s }
    function step(given: "todo" | "doing" | "done", on: boolean): void {
        let picked = on ? given : -1
        if (typeof picked === "string") onlyKnown(picked)
    }
}
