// expect: accept
// Boundary: what the declarations give as unknown is narrowed before it is used.
import Ljos.Ui
import "B01Declared.js" as Lib
Item {
    property string label: ""
    function show(text: string): void {
        const v = Lib.load(text)
        if (typeof v === "object" && v !== null && "title" in v && typeof v.title === "string") { label = v.title }
    }
}
