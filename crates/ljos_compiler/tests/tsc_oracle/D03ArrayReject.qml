// expect: reject-subset ts-no-any
// Decoder: TypeScript's `Array.isArray` gives `any[]`, so it lets `v[0].size` by; the subset's `unknown[]` does not.
import Ljos.Ui
Item {
    property real shown: 0
    function take(v: unknown): void {
        if (Array.isArray(v)) { shown = v.length + v[0].size }
    }
}
