// expect: reject-type 7 TS2322
// Shape: a label given a number where its text goes.
// After corpus/qtdoc/examples/demos/dice/Main.qml:124.
import Ljos.Ui
Item {
    property real friction: 0.5
    Text { text: friction * 100 }
}
