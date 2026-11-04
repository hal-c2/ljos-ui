// expect: accept
// Decoder: `filter` with a guard keeps what the guard says it found.
import Ljos.Ui
import "D04Guard.ts" as Logic
Item {
    property real shown: 0
    function take(all: readonly unknown[]): void { const rows: readonly Logic.Row[] = all.filter(Logic.isRow); shown = rows.length }
}
