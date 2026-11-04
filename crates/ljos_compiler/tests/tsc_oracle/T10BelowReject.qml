// expect: reject-type 9 TS2345
// Shape: a written negative count widens to a number, so it is no longer that one count.
import Ljos.Ui
Item {
    property real shown: 0
    function onlyBelow(n: -1): void { shown = n }
    function step(given: "todo" | "doing" | "done", on: boolean): void {
        let picked = on ? given : -1
        if (typeof picked === "number") onlyBelow(picked)
    }
}
