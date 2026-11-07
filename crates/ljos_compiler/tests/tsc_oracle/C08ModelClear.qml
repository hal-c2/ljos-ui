// expect: accept
// Shape: rows taken out of a model from the end.
// After corpus/qtdoc/examples/demos/calqlatr/content/Display.qml:84.
import Ljos.Ui; import Ljos.Core
Item {
    ListModel { id: history }
    function trim(keep: int): void {
        for (let i = history.count - 1; i >= keep; i--)
            history.remove(i)
    }
}
