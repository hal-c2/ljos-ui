// expect: accept
// Shape: a binding follows a pointer area's state.
// After corpus/qtdoc/examples/demos/hangman/qml/StoreItem.qml.
import Ljos.Ui
Rectangle {
    fill: area.containsMouse ? "#eeeeee" : "#ffffff"
    PointerArea { id: area; hoverEnabled: true }
}
