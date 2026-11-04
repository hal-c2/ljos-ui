// expect: accept
// Decoder: a guard's body gives the boolean of what it found.
import Ljos.Ui
Item {
    property string shown: ""
    function isTag(v: unknown): v is string { return typeof v === "string" }
    function take(v: unknown): void { if (isTag(v)) { shown = v } }
}
