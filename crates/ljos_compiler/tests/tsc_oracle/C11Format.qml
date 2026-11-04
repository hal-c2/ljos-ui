// expect: accept
// Shape: a label formats a number into translated text.
// After corpus/qtdoc/examples/demos/dice/Main.qml:124.
import Ljos.Ui
Item {
    property real friction: 0.5
    Text { text: qsTr("Static friction: %1").arg(friction.toFixed(2)) }
}
