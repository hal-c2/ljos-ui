// expect: accept
// Shape: a function moves a flow to a named state.
// After corpus/qtdoc/examples/demos/coffee/ApplicationFlow.qml:38.
import Ljos.Ui
Item {
    id: flow
    states: [
        State { name: "Selection" },
        State { name: "Settings" }
    ]
    function showSettings(): void { flow.state = "Settings" }
}
