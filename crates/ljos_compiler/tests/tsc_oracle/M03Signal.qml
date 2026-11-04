// expect: accept
// A signal whose parameter is a type the module exports: its handler is
// given that type.
import Ljos.Ui
import "M03Signal.ts" as Logic
Item {
    property real shown: 0
    signal chosen(row: Logic.Row)
    onChosen: row => { shown = row.count }
    function choose(): void { chosen({ tag: "a", count: 1 }) }
}
