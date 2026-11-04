// expect: reject-type 7 TS2375
// Note 11 §3.5 with the member set to what may be undefined: exactOptionalPropertyTypes refuses it too.
import Ljos.Ui
Item {
    property string shown: ""
    function rename(given: string | undefined): void {
        const row: { name?: string } = { name: given }
        shown = row.name ?? ""
    }
}
