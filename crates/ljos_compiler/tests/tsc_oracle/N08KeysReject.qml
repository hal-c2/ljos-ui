// expect: reject-subset ts-condition-boolean
// Note 11 §3.8 as JavaScript wrote it: a masked flag tested for truth.
import Ljos.Ui
Item {
    property int chosen: -1
    function keyed(key: int, modifiers: int): bool {
        const ctrl = modifiers & Modifier.Control
        if (ctrl && key >= Key.Digit1 && key <= Key.Digit9) {
            chosen = key - Key.Digit1
            return true
        }
        return false
    }
}
