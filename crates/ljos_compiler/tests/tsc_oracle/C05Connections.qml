// expect: accept
// Shape: Connections reacts to a signal of another object, a function per handler.
// After corpus/qtdoc/examples/demos/lightningviewer/LightningViewerModule/MapLayersDrawer.qml:92.
import Ljos.Ui
Item {
    id: drawer
    property real offset: 0
    signal dragged(by: real)
    Connections {
        target: drawer
        function onDragged(by: real) { drawer.offset = drawer.offset + by }
    }
}
