// expect: reject-subset ts-no-var
// Shape: Component.onCompleted setup that declares with `var`.
// After corpus/qtdoc/examples/demos/android/feature-delivery/fdmaploader/Main.qml:252.
import Ljos.Ui; import Ljos.Core
Item {
    property string title: ""
    Component.onCompleted: {
        var first = qsTr("Maps")
        title = first
    }
}
