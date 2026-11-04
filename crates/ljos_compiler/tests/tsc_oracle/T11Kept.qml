// expect: accept
// Shape: a const of a status or a written count, copied into a variable; the copy widens the count only.
import Ljos.Ui
Item {
    property string shown: ""
    function onlyKnown(s: "todo" | "doing" | "done"): void { shown = s }
    function step(given: "todo" | "doing" | "done", on: boolean): void {
        const kept = on ? given : -1
        let again = kept
        if (typeof again === "string") onlyKnown(again)
    }
}
