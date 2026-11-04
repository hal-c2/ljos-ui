// expect: accept
// Note 11 §3.8: a key handler; Ljos.Ui has no Keys yet, so the handler is a
// function of the key and its modifiers.
import Ljos.Ui
Item {
    property int chosen: -1
    property int moved: 0
    function keyed(key: int, modifiers: int): bool {
        const ctrl = (modifiers & Qt.ControlModifier) !== 0
        if (ctrl && key >= Qt.Key_1 && key <= Qt.Key_9) {
            chosen = key - Qt.Key_1
        } else if (key === Qt.Key_Down) {
            moved += 1
        } else {
            return false
        }
        return true
    }
}
