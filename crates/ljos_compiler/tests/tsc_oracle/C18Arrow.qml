// expect: accept
// Shape: an arrow handler given the signal's argument, which it does not need.
// After corpus/qtdoc/examples/demos/FX_Material_Showroom/content/Figma_Assets/MouseScroll.qml:67.
import Ljos.Ui
PointerArea {
    property bool keysEnabled: true
    property int held: 0
    onPressAndHold: mouse => { if (keysEnabled) held += 1 }
}
