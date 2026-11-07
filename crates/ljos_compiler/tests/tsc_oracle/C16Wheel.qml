// expect: accept
// Shape: a Connections handler of a pointer area's wheel signal.
// After corpus/qtdoc/examples/demos/car-configurator/content/PinchWidget.qml:32.
import Ljos.Ui; import Ljos.Core
Item {
    property int turns: 0
    PointerArea { id: area }
    Connections {
        target: area
        function onWheel(wheel: QtObject) { turns += 1 }
    }
}
