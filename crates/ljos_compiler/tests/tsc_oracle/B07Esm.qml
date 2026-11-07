// expect: accept
// Boundary: an .mjs file is called by the .d.mts beside it, as a .js file is by its .d.ts.
import Ljos.Ui
import "B07Esm.mjs" as Lib
Item {
    property real total: 0
    property string label: ""
    function show(): void { total = Lib.add(1, 2); label = Lib.unit }
}
