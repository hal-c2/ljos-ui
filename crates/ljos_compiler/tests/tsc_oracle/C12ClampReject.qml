// expect: reject-type
// Shape: a clamp helper called with a string.
// After corpus/qtdoc/examples/demos/FX_Material_Showroom/imports/QtQuick/Studio/Components/ArcItem.qml:291.
import Ljos.Ui
Item {
    property real level: 0
    function clamp(num: real, min: real, max: real): real { return Math.max(min, Math.min(num, max)) }
    function reset(): void { level = clamp("0", 0, 1) }
}
