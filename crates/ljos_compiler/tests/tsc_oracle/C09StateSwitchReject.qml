// expect: reject-type
// Shape: a flow moved to a state given by number.
// After corpus/qtdoc/examples/demos/coffee/ApplicationFlow.qml:38.
import Ljos.Ui
Item {
    id: flow
    states: [ State { name: "Settings" } ]
    function showSettings(): void { flow.state = 1 }
}
