// expect: reject-subset ts-condition-boolean
// Note 11 §3.4 with JavaScript's truthiness: a count where a test goes.
import Ljos.Ui
Item {
    property string status: "proposed"
    property int kept: 0
    function runLabel(): string {
        if (status !== "proposed")
            return status
        return kept ? qsTr("%1 changes").arg(kept) : qsTr("Status update")
    }
}
