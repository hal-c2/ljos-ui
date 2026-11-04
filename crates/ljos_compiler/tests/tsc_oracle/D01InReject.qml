// expect: reject-type 7 TS18046
// Decoder: `in` on what is still unknown: TypeScript looks only into an object.
import Ljos.Ui
Item {
    property unknown given
    property string shown: ""
    function take(): void { if ("tag" in given) { shown = "found" } }
}
