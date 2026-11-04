// expect: accept
// Shape: a switch over the key that was pressed moves by a step.
// After corpus/qtdoc/examples/demos/FX_Material_Showroom/content/Figma_Assets/MouseScroll.qml:148.
import Ljos.Ui
Item {
    property real dx: 0
    property real dy: 0
    function pressed(key: int): bool {
        switch (key) {
        case Qt.Key_W: dy -= 1; return true
        case Qt.Key_S: dy += 1; return true
        case Qt.Key_A: dx -= 1; return true
        case Qt.Key_D: dx += 1; return true
        default: return false
        }
    }
}
