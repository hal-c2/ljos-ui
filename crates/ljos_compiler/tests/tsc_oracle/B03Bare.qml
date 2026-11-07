// expect: accept
// Boundary: the same call, once B01Declared.d.ts says what it takes and gives.
import Ljos.Ui
import "B01Declared.js" as Lib
Item {
    property real total: 0
    function show(): void { total = Lib.add(1, 2) }
}
