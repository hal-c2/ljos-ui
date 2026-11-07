// expect: accept
// Shape: a timer hides what it showed.
// After corpus/qtdoc/examples/demos/android/feature-delivery/fdmaploader/Main.qml:113.
import Ljos.Ui; import Ljos.Core
Item {
    Rectangle { id: toast; visible: true }
    Timer { interval: 3000; running: toast.visible; onTriggered: toast.visible = false }
}
