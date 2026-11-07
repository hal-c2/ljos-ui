// expect: reject-subset ts-no-any
// Decoder: TypeScript's `JSON.parse` gives `any`, so it lets `.count` by; the subset's `unknown` does not.
import Ljos.Ui
Item {
    property string text: ""
    property real shown: 0
    function take(): void { shown = JSON.parse(text).count }
}
