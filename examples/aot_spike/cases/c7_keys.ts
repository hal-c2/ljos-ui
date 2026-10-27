// c7: a key handler (note 11 §3.8). The palette's calls are writes here;
// `accepted` stands for the event's.
function leaveSubmenu(): boolean {
    if (root.depth === 0)
        return false;
    root.depth -= 1;
    return true;
}

export function root__onKeyPressed(key: number, modifiers: number): void {
    const ctrl = (modifiers & Qt.ControlModifier) !== 0;
    if (ctrl && key >= Qt.Key_1 && key <= Qt.Key_9) {
        root.ran = key - Qt.Key_1;
    } else if (key === Qt.Key_Down) {
        root.cursor += 1;
    } else if (key === Qt.Key_Backspace && field.text.length === 0) {
        if (!leaveSubmenu())
            return;
    } else {
        return;
    }
    root.accepted = true;
}
