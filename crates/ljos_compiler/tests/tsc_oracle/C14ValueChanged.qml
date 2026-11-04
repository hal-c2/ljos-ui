// expect: accept
// Shape: a change handler block that compares and stores.
// After corpus/qtdoc/examples/demos/qtjennydemo/Main.qml:101.
import Ljos.Ui
Item {
    property real value: 0
    property real last: 0
    property bool rising: false
    onValueChanged: {
        rising = value > last
        last = value
    }
}
