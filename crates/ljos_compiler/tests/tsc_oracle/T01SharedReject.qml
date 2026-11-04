// expect: reject-subset ts-variance
// Note 11 §3.2 with the place mutable: a string pushed through it would land in a number[].
import Ljos.Ui
Item {
    property int seen: 0
    function show(selected: number[]): void {
        const all: (number | string)[] = selected
        seen = all.length
    }
}
