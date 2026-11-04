// expect: reject-type 10 TS2345
// Shape: a const of a status or a written name, copied into a variable; the name widens to a string.
import Ljos.Ui
Item {
    property string shown: ""
    function onlyKnown(s: "todo" | "doing" | "done"): void { shown = s }
    function step(given: "todo" | "doing" | "done", on: boolean): void {
        const kept = on ? given : "later"
        let again = kept
        if (typeof again === "string") onlyKnown(again)
    }
}
