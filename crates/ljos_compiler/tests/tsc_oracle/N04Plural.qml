// expect: accept
// Note 11 §3.4: a formatting helper with a plural.
import Ljos.Ui; import Ljos.Core
Item {
    property string status: "proposed"
    property int kept: 0
    property bool health: false
    function runLabel(): string {
        if (status !== "proposed")
            return status
        if (kept === 0 && health)
            return t`Status update`
        return plural(kept, { one: "1 change", other: "# changes" })
    }
}
