// expect: reject-type 11 TS2322
// Shape: a Connections handler that stores its number argument in a string.
// After corpus/qtdoc/examples/demos/lightningviewer/LightningViewerModule/MapLayersDrawer.qml:92.
import Ljos.Ui; import Ljos.Core
Item {
    id: drawer
    property string said: ""
    signal dragged(by: real)
    Connections {
        target: drawer
        function onDragged(by: real) { drawer.said = by }
    }
}
