// expect: accept
// Decoder: what `JSON.parse` gives is decoded before it is read, and `JSON.stringify` gives a string.
import Ljos.Ui
import "D04Guard.ts" as Logic
Item {
    property string text: ""
    property real shown: 0
    property string saved: ""
    function take(): void {
        const v = JSON.parse(text)
        if (Logic.isRow(v)) { shown = v.count; saved = JSON.stringify(v) }
    }
}
