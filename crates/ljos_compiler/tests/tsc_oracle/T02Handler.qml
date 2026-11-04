// expect: accept
// Note 11 §3.6: the completion callback takes everything its place may be given.
import Ljos.Ui
Item {
    property string shown: ""
    function fetch(done: { finish(answer: string | number): void }): void { done.finish("ok") }
    function start(): void {
        fetch({ finish: (answer: string | number): void => { shown = String(answer) } })
    }
}
