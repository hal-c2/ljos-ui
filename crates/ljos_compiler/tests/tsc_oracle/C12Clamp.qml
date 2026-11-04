// expect: accept
// Shape: a helper clamps a number between two others.
// After corpus/qtdoc/examples/demos/FX_Material_Showroom/imports/QtQuick/Studio/Components/ArcItem.qml:291.
import Ljos.Ui
Item {
    property real level: 0
    function clamp(num: real, min: real, max: real): real { return Math.max(min, Math.min(num, max)) }
    function nudge(by: real): void { level = clamp(level + by, 0, 1) }
}
