// expect: reject-subset ts-forbidden
// logic.js:44's for…in, typed: TypeScript accepts it, the subset has no for…in.
import Ljos.Ui
Item {
    property int total: 0
    function sum(waves: readonly number[]): void {
        let all = 0
        for (const i in waves) { all += 1 }
        total = all
    }
}
