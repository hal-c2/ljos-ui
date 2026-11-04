// expect: accept
// Note 11 §3.3: a label for each status, one case per member.
import Ljos.Ui
Item {
    property string shown: ""
    function statusLabel(status: "todo" | "doing" | "blocked" | "done"): string {
        switch (status) {
        case "todo": return qsTr("To do")
        case "doing": return qsTr("In progress")
        case "blocked": return qsTr("Blocked")
        case "done": return qsTr("Done")
        }
    }
    Component.onCompleted: shown = statusLabel("doing")
}
