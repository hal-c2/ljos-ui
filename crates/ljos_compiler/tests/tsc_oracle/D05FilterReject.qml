// expect: reject-type 7 TS2322
// Decoder: `filter` with a guard of another type keeps that type.
import Ljos.Ui
import "D04Guard.ts" as Logic
Item {
    property real shown: 0
    function take(all: readonly unknown[]): void { const rows: readonly Logic.Row[] = all.filter(Logic.isTag); shown = rows.length }
}
