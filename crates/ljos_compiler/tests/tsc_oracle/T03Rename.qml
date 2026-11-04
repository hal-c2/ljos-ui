// expect: accept
// Note 11 §3.5: a record updated by building a new one, its readonly id kept.
import Ljos.Ui
Item {
    property string shown: ""
    function rename(row: { readonly id: number; name: string }, name: string): void {
        const next: { readonly id: number; name: string } = { ...row, name }
        shown = next.name
    }
}
