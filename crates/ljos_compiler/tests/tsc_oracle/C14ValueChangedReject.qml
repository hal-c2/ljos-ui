// expect: reject-subset ts-no-null
// Shape: a change handler that clears a value with `null`.
// After corpus/qtdoc/examples/demos/qtjennydemo/Main.qml:101.
import Ljos.Ui
Item {
    property real value: 0
    property bool cleared: false
    onValueChanged: {
        const before = null
        cleared = before === null
    }
}
