// expect: accept
// Shape: a button's state follows press and release.
// After corpus/qtdoc/examples/demos/hangman/qml/SimpleButton.qml:49.
import Ljos.Ui
Rectangle {
    id: button
    states: [ State { name: "PRESSED" } ]
    PointerArea {
        anchors.fill: parent
        onPressed: button.state = "PRESSED"
        onReleased: button.state = ""
    }
}
