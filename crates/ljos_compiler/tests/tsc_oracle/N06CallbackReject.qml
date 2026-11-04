// expect: reject-type
// Note 11 §3.6 calling the optional callback as if it were always there.
import Ljos.Ui
Item {
    property int busy: 0
    property string refusal: ""
    function finish(result: string, error: string | undefined,
                    done?: (result: string, error: string | undefined) => void): void {
        busy -= 1
        if (error !== undefined)
            refusal = error
        done(result, error)
    }
}
