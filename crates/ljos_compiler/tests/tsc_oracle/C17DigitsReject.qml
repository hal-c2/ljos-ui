// expect: reject-type 9 TS2322
// Shape: a number shown with the digits that fit, returned as a number.
// After corpus/qtdoc/examples/demos/calqlatr/content/Display.qml:120.
import Ljos.Ui
Item {
    property int maxDigits: 8
    function shown(num: real): string {
        if (num.toString().length <= maxDigits)
            return num
        return num.toFixed(maxDigits - 2)
    }
}
