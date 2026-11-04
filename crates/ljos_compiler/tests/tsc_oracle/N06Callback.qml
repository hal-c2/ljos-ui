// expect: accept
// Note 11 §3.6: a call with an optional completion callback.
import Ljos.Ui
Item {
    property int busy: 0
    property string refusal: ""
    function finish(result: string, error: string | undefined,
                    done?: (result: string, error: string | undefined) => void): void {
        busy -= 1
        if (error !== undefined)
            refusal = error
        done?.(result, error)
    }
    function ask(method: string, done?: (result: string, error: string | undefined) => void): void {
        refusal = ""
        busy += 1
        finish(method, undefined, done)
    }
}
