// expect: reject-type 6 TS2366
// Note 11 §3.3 with a member left out: the switch does not cover the union.
import Ljos.Ui
Item {
    property string shown: ""
    function statusLabel(status: "todo" | "doing" | "blocked" | "done"): string {
        switch (status) {
        case "todo": return qsTr("To do")
        case "doing": return qsTr("In progress")
        case "done": return qsTr("Done")
        }
    }
    Component.onCompleted: shown = statusLabel("doing")
}
