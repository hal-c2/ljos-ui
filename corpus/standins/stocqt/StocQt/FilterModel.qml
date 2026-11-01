// Stands in for the QSortFilterProxyModel that StockEngine gives as
// filterModel: the rows of the list of stocks whose "filter" (stockId and
// name) has the text looked for in it, in either case.
//
// Of QSortFilterProxyModel's own interface there is setFilterFixedString,
// which is what the example calls.
import QtQml
import QtQml.Models

ListModel {
    // The StockListModel the rows are of.
    property var sourceModel: null
    property string fixedString: ""

    function accepts(row) {
        return row.filter.toLowerCase().includes(fixedString.toLowerCase())
    }

    // The rows that no longer match go and the ones that now do come, each
    // at its place; the others stay as they are.
    function setFilterFixedString(pattern) {
        fixedString = pattern
        let at = 0
        for (const row of sourceModel.rows()) {
            const shown = at < count && get(at).stockId === row.stockId
            const wanted = accepts(row)
            if (shown && !wanted)
                remove(at)
            else if (!shown && wanted)
                insert(at++, row)
            else if (shown)
                at++
        }
    }

    // The source was reset: so is this.
    function reset() {
        clear()
        for (const row of sourceModel.rows()) {
            if (accepts(row))
                append(row)
        }
    }
}
