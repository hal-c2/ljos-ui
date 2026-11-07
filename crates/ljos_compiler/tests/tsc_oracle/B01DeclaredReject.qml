// expect: reject-type 7 TS2345
// Boundary: a call its declarations do not take is wrong where the QML file makes it.
import Ljos.Ui
import "B01Declared.js" as Lib
Item {
    property real total: 0
    function show(): void { total = Lib.add("1", 2) }
}
