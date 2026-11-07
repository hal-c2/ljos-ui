// expect: reject-type 8 TS18046
// Boundary: what the declarations give as unknown has no members until it is narrowed.
import Ljos.Ui
import "B01Declared.js" as Lib
Item {
    property string label: ""
    function show(text: string): void {
        const v = Lib.load(text); label = v.title
    }
}
