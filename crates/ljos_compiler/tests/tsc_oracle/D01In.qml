// expect: accept
// Decoder: `"tag" in v` once v is known to be an object gives `v.tag: unknown`, narrowed by `typeof`; `null` may be
// ruled out before `typeof` or by an early return.
import Ljos.Ui
Item {
    property unknown given
    property string shown: ""
    function take(): void {
        if (typeof given === "object" && given !== null && "tag" in given && typeof given.tag === "string") { shown = given.tag }
    }
    function nullFirst(v: unknown): string {
        if (v !== null && typeof v === "object" && "tag" in v && typeof v.tag === "string") { return v.tag }
        return ""
    }
    function earlyReturn(v: unknown): string {
        if (v === null) { return "" }
        if (typeof v === "object" && "tag" in v && typeof v.tag === "string") { return v.tag }
        return ""
    }
}
