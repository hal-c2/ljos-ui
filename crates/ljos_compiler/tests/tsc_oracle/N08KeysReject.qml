// expect: reject-subset ts-condition-boolean
// Note 11 §3.8 as JavaScript wrote it: a masked flag tested for truth.
import Ljos.Ui
Item {
    property int chosen: -1
    function keyed(key: int, modifiers: int): bool {
        const ctrl = modifiers & Qt.ControlModifier
        if (ctrl && key >= Qt.Key_1 && key <= Qt.Key_9) {
            chosen = key - Qt.Key_1
            return true
        }
        return false
    }
}
