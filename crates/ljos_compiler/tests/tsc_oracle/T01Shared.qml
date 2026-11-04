// expect: accept
// Note 11 §3.2: the toggled list read where any id may be seen; a readonly place takes it.
import Ljos.Ui
Item {
    property int seen: 0
    function count(ids: readonly (number | string)[]): number { return ids.length }
    function show(selected: number[]): void {
        const all: readonly (number | string)[] = selected
        seen = count(all)
    }
}
