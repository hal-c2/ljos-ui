import QtQuick

// The dynamic versions of two cases, for a run that swaps its compiled
// code for them (lib.rs `swap_to_dynamic`): the IR a build ships next to
// the native code, made into an object only when it is needed.
Item {
    property var r
    property var b

    function toggled(list, value) {
        return list.includes(value) ? list.filter(each => each !== value) : [...list, value];
    }

    function onPicked(value) {
        r.selected = toggled(r.selected, value);
    }

    function bindProgress() {
        r.progress = Qt.binding(() => r.total > 0 ? Math.min(1, r.done / r.total) * b.width : 0);
    }
}
