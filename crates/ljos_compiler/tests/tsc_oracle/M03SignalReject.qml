// expect: reject-type 9 TS2322
// A signal whose parameter is a type the module exports: its handler
// reads a string field into a number.
import Ljos.Ui
import "M03Signal.ts" as Logic
Item {
    property real shown: 0
    signal chosen(row: Logic.Row)
    onChosen: row => { shown = row.tag }
    function choose(): void { chosen({ tag: "a", count: 1 }) }
}
