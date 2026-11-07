// expect: accept
// Shape: a list model gets a row for each result.
// After corpus/qtdoc/examples/demos/calqlatr/content/Display.qml:20.
import Ljos.Ui; import Ljos.Core
Item {
    ListModel { id: history }
    function record(operand: string, operator: string): void {
        history.append({ operand: operand, operator: operator })
    }
}
