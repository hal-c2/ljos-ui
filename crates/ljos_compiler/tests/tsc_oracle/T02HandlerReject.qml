// expect: reject-subset ts-variance
// Note 11 §3.6 with the callback taking less than its place: TypeScript's methods are bivariant, the subset's are not.
import Ljos.Ui
Item {
    property string shown: ""
    function fetch(done: { finish(answer: string | number): void }): void { done.finish(1) }
    function start(): void {
        fetch({ finish: (answer: string): void => { shown = answer } })
    }
}
