// expect: reject-subset ts-loose-equality
// Shape: rows taken out of a model, the end found with `==`.
// After corpus/qtdoc/examples/demos/calqlatr/content/Display.qml:84.
import Ljos.Ui
Item {
    ListModel { id: history }
    function trimAll(): void {
        while (!(history.count == 0))
            history.remove(history.count - 1)
    }
}
