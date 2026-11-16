// expect: accept
// Shape: a label formats a number into translated text.
// After corpus/qtdoc/examples/demos/dice/Main.qml:124.
import Ljos.Ui; import Ljos.Core
Item {
    property real friction: 0.5
    Text { text: t`Static friction: ${friction.toFixed(2)}` }
}
