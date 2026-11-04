// expect: reject-type 9 TS2345
// Shape: a count written with its sign, in a variable, widens to a number.
import Ljos.Ui
Item {
    property real shown: 0
    function onlyOne(n: 1): void { shown = n }
    function step(): void {
        let p = +1
        onlyOne(p)
    }
}
