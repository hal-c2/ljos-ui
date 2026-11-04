// expect: reject-subset ts-readonly
// Note 11 §3.5 through a mutable alias: TypeScript lets a readonly member be given where a
// mutable one goes, so the write lands in the readonly row; the subset keeps it readonly.
import Ljos.Ui
Item {
    property string shown: ""
    function rename(row: { readonly id: number; name: string }, name: string): void {
        const next: { id: number; name: string } = row
        next.id = 2
        shown = name
    }
}
