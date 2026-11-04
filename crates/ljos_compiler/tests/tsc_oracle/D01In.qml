// expect: accept
// Decoder: `"tag" in v` once v is known to be an object gives `v.tag: unknown`, narrowed by `typeof`.
import Ljos.Ui
Item {
    property unknown given
    property string shown: ""
    function take(): void {
        if (typeof given === "object" && given !== null && "tag" in given && typeof given.tag === "string") { shown = given.tag }
    }
}
