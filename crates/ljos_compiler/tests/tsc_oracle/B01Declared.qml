// expect: accept
// Boundary: a JavaScript file with declarations beside it is called by them.
import Ljos.Ui
import "B01Declared.js" as Lib
Item {
    property real total: 0
    property string label: ""
    function show(): void { total = Lib.add(1, 2); label = Lib.unit }
}
