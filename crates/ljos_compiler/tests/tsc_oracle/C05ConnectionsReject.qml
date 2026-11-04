// expect: reject-type
// Shape: a Connections handler that stores its number argument in a string.
// After corpus/qtdoc/examples/demos/lightningviewer/LightningViewerModule/MapLayersDrawer.qml:92.
import Ljos.Ui
Item {
    id: drawer
    property string said: ""
    signal dragged(by: real)
    Connections {
        target: drawer
        function onDragged(by: real) { drawer.said = by }
    }
}
