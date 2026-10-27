import QtQuick
import QtQml.Models

// The eight cases of docs/research/13-aot-spike.md as the dynamic tiers run
// them: tier 1 where the VM takes the expression, QuickJS for the rest.
// Shell.qml is this file with every binding and handler of a case taken
// out, for a back end that installs its own.
Item {
    id: root
    objectName: "root"

    property real done: 3
    property real total: 10
    property bool stopMode: false
    property string lastMode: ""
    property string status: "todo"
    property string runStatus: "proposed"
    property var changes: [false, true, false]
    property bool health: false
    property var selected: []
    property int ran: -1
    property int cursor: 0
    property int depth: 0
    property bool accepted: false
    property var items: []
    property int syncs: 0

    signal clicked()
    signal dispatched(string action)
    signal picked(string value)
    signal keyPressed(int key, int modifiers)

    // c1: a binding that reads several properties.
    property real progress: total > 0 ? Math.min(1, done / total) * bar.width : 0
    // c4: a branch on a mode.
    property string statusText: statusLabel(status)
    // c5: string formatting with a plural.
    property string runText: runLabel(runStatus, changes, health)

    function statusLabel(status) {
        switch (status) {
        case "todo": return qsTr("To do");
        case "doing": return qsTr("In progress");
        case "blocked": return qsTr("Blocked");
        case "done": return qsTr("Done");
        case "dropped": return qsTr("Dropped");
        default: return status;
        }
    }

    function runLabel(status, changes, health) {
        if (status !== "proposed")
            return statusLabel(status);
        const kept = changes.filter(dismissed => !dismissed).length;
        if (kept === 0 && health)
            return qsTr("Status update");
        return kept === 1 ? qsTr("1 change") : qsTr("%1 changes").arg(kept);
    }

    function toggled(list, value) {
        return list.includes(value) ? list.filter(each => each !== value) : [...list, value];
    }

    function leaveSubmenu() {
        if (root.depth === 0)
            return false;
        root.depth -= 1;
        return true;
    }

    // c2: a one-line handler (it emits one of two signals).
    onClicked: root.stopMode ? root.dispatched("editor.interrupt") : editor.submitted("foreground")

    // c6: a list filter.
    onPicked: value => root.selected = toggled(root.selected, value)

    // c7: a key handler with several writes.
    onKeyPressed: (key, modifiers) => {
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

    // c8: keeping a list model in step.
    onItemsChanged: {
        const items = root.items;
        const shown = new Set(items);
        for (let i = cards.count - 1; i >= 0; --i) {
            if (shown.has(cards.get(i).toastId))
                continue;
            cards.remove(i);
        }
        for (let i = 0; i < items.length; ++i) {
            let at = -1;
            for (let j = i; j < cards.count; ++j) {
                if (cards.get(j).toastId === items[i]) {
                    at = j;
                    break;
                }
            }
            if (at < 0)
                cards.insert(i, { toastId: items[i] });
            else if (at !== i)
                cards.move(at, i, 1);
        }
        root.syncs += 1;
    }

    Item {
        id: bar
        objectName: "bar"
        width: 240
    }

    Item {
        id: editor
        objectName: "editor"
        signal submitted(string mode)
        // c3: a one-line handler that writes a property.
        onSubmitted: mode => root.lastMode = mode
    }

    Item {
        id: field
        objectName: "field"
        property string text: ""
    }

    ListModel {
        id: cards
        objectName: "cards"
    }
}
