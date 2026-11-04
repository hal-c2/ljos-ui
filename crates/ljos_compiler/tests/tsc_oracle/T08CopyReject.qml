// expect: reject-type 10 TS2345
// Shape: a status started from a written literal, through a const: a variable widens it to a string.
import Ljos.Ui
Item {
    property string shown: ""
    function onlyKnown(s: "todo" | "doing" | "done"): void { shown = s }
    function step(): void {
        const first = "todo"
        let status = first
        onlyKnown(status)
    }
}
