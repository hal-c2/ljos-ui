// expect: reject-type 9 TS2345
// Shape: a value that is a status or a written name; the name widens to a string, so it is no status.
import Ljos.Ui
Item {
    property string shown: ""
    function onlyKnown(s: "todo" | "doing" | "done"): void { shown = s }
    function step(given: "todo" | "doing" | "done", on: boolean): void {
        let picked = on ? given : "later"
        if (typeof picked === "string") onlyKnown(picked)
    }
}
