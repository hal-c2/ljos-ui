// expect: accept
// Shape: a number shown with the digits that fit.
// After corpus/qtdoc/examples/demos/calqlatr/content/Display.qml:120.
import Ljos.Ui
Item {
    property int maxDigits: 8
    function shown(num: real): string {
        const text = num.toString()
        if (text.length <= maxDigits)
            return text
        return num.toFixed(maxDigits - 2)
    }
}
