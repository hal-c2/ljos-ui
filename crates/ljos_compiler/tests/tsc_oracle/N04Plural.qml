// expect: accept
// Note 11 §3.4: a formatting helper with a plural.
import Ljos.Ui
Item {
    property string status: "proposed"
    property int kept: 0
    property bool health: false
    function runLabel(): string {
        if (status !== "proposed")
            return status
        if (kept === 0 && health)
            return qsTr("Status update")
        return kept === 1 ? qsTr("1 change") : qsTr("%1 changes").arg(kept)
    }
}
