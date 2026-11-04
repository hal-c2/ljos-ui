// expect: reject-type 6 TS2339
// Shape: a binding that reads a member the pointer area does not have.
// After corpus/qtdoc/examples/demos/hangman/qml/StoreItem.qml.
import Ljos.Ui
Rectangle {
    fill: area.hovered ? "#eeeeee" : "#ffffff"
    PointerArea { id: area; hoverEnabled: true }
}
