// expect: accept
// Shape: each wave of a game read in turn, by for…of.
// After corpus/qtdoc/examples/demos/maroon/content/logic.js:44.
import Ljos.Ui
Item {
    property int total: 0
    function sum(waves: readonly number[]): void {
        let all = 0
        for (const wave of waves) { all += wave }
        total = all
    }
}
