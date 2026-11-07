// expect: accept
// Note 11 §3.9: a list model kept in step with a list, a Map for the index.
import Ljos.Ui; import Ljos.Core
Item {
    property list<string> shown: []
    ListModel { id: cards }
    function sync(byId: Map<string, number>, ids: readonly string[]): void {
        for (let i = cards.count - 1; i >= 0; --i) {
            const id = ids[i]
            if (id === undefined || shown.includes(id))
                continue
            cards.remove(i)
            byId.delete(id)
        }
    }
}
