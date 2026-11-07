// expect: accept
// Shape: Component.onCompleted sets the first values up.
// After corpus/qtdoc/examples/demos/android/feature-delivery/fdmaploader/Main.qml:252.
import Ljos.Ui; import Ljos.Core
Item {
    property string title: ""
    property int shown: 0
    Component.onCompleted: {
        title = qsTr("Maps")
        shown = 1
    }
}
