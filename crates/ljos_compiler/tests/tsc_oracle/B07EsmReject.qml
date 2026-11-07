// expect: reject-type 7 TS2345
// Boundary: a call the .d.mts beside an .mjs file does not take is wrong where the QML file makes it.
import Ljos.Ui
import "B07Esm.mjs" as Lib
Item {
    property real total: 0
    function show(): void { total = Lib.add("1", 2) }
}
