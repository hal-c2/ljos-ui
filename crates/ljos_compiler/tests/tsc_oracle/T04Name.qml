// expect: accept
// Note 11 §3.5: the record updated, the optional member left out when there is none.
import Ljos.Ui
Item {
    property string shown: ""
    function rename(given: string | undefined): void {
        const row: { name?: string } = given === undefined ? {} : { name: given }
        shown = row.name ?? ""
    }
}
