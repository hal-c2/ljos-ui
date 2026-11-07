// expect: reject-subset ts-no-var
// Note 11 §3.9 with the loop's counter declared by `var`.
import Ljos.Ui; import Ljos.Core
Item {
    property list<string> shown: []
    ListModel { id: cards }
    function sync(ids: readonly string[]): void {
        for (var i = cards.count - 1; i >= 0; --i) {
            const id = ids[i]
            if (id === undefined || shown.includes(id))
                continue
            cards.remove(i)
        }
    }
}
