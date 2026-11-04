// expect: accept
// A property and a function's parameter of a type the module exports.
import Ljos.Ui
import "M05Type.ts" as Logic
Item {
    property list<Logic.Row> kept: []
    function keep(row: Logic.Row): void { kept = [row] }
}
