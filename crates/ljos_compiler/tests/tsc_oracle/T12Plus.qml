// expect: accept
// Shape: a count written with its sign is that count, as a const keeps it.
import Ljos.Ui
Item {
    property real shown: 0
    function onlyOne(n: 1): void { shown = n }
    function step(): void {
        const p = +1
        onlyOne(p)
    }
}
